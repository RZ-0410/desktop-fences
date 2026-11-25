//! 搬运日志：记录每次文件移动的源与目标，用于「放回原位置」和「撤销上次整理」。
//!
//! 每行 TSV：`batch \t ts \t op \t src \t dst`
//! 文件名里的 Tab/换行/百分号做转义，避免把一行拆错。

use std::fs::OpenOptions;
use std::io::Write;
use std::path::{Path, PathBuf};

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Op {
    Move,
    Undo,
}

impl Op {
    fn as_str(self) -> &'static str {
        match self {
            Op::Move => "M",
            Op::Undo => "U",
        }
    }
}

pub struct Entry {
    pub batch: u64,
    pub ts: u64,
    pub op: Op,
    pub src: PathBuf,
    pub dst: PathBuf,
}

fn esc(s: &str) -> String {
    s.replace('%', "%25")
        .replace('\t', "%09")
        .replace('\n', "%0A")
        .replace('\r', "%0D")
}

fn unesc(s: &str) -> String {
    s.replace("%0D", "\r")
        .replace("%0A", "\n")
        .replace("%09", "\t")
        .replace("%25", "%")
}

pub fn append(op: Op, batch: u64, src: &Path, dst: &Path) -> std::io::Result<()> {
    let dir = crate::config::data_dir();
    std::fs::create_dir_all(&dir)?;
    let path = crate::config::journal_path();
    let mut f = OpenOptions::new().create(true).append(true).open(path)?;
    writeln!(
        f,
        "{}\t{}\t{}\t{}\t{}",
        batch,
        crate::now_stamp(),
        op.as_str(),
        esc(&src.to_string_lossy()),
        esc(&dst.to_string_lossy())
    )
}

pub fn read_all() -> Vec<Entry> {
    let text = match std::fs::read_to_string(crate::config::journal_path()) {
        Ok(t) => t,
        Err(_) => return Vec::new(),
    };
    let mut out = Vec::new();
    for line in text.lines() {
        let parts: Vec<&str> = line.split('\t').collect();
        if parts.len() != 5 {
            continue;
        }
        let batch = parts[0].parse().unwrap_or(0);
        let ts = parts[1].parse().unwrap_or(0);
        let op = match parts[2] {
            "U" => Op::Undo,
            _ => Op::Move,
        };
        out.push(Entry {
            batch,
            ts,
            op,
            src: PathBuf::from(unesc(parts[3])),
            dst: PathBuf::from(unesc(parts[4])),
        });
    }
    out
}

/// 路径比较键：统一反斜杠 + 小写。
/// 不统一的话 "E:/a/b" 和 "E:////a////b" 会被当成两个文件（实测踩过）。
fn path_key(p: &Path) -> String {
    p.to_string_lossy().replace('/', "\\").to_lowercase()
}

/// 查一个文件当初从哪儿搬来的（最后一次 MOVE 的 src）。
pub fn origin_of(path: &Path) -> Option<PathBuf> {
    let key = path_key(path);
    read_all()
        .iter()
        .rev()
        .find(|e| e.op == Op::Move && path_key(&e.dst) == key)
        .map(|e| e.src.clone())
}

/// 撤销最后一个可撤销批次。返回 (成功数, 跳过数)。
pub fn undo_last() -> (usize, usize) {
    let entries = read_all();
    let mut batches: Vec<u64> = entries
        .iter()
        .filter(|e| e.op == Op::Move)
        .map(|e| e.batch)
        .collect();
    batches.sort_unstable();
    batches.dedup();
    batches.reverse();

    for b in batches {
        let already = entries.iter().any(|e| e.op == Op::Undo && e.batch == b);
        if already {
            continue;
        }
        let mut ok = 0usize;
        let mut skip = 0usize;
        let moves: Vec<&Entry> = entries
            .iter()
            .filter(|e| e.batch == b && e.op == Op::Move)
            .collect();
        for e in moves.iter().rev() {
            if !e.dst.exists() {
                skip += 1;
                continue;
            }
            let target = unique(&e.src);
            if let Some(parent) = target.parent() {
                let _ = std::fs::create_dir_all(parent);
            }
            match std::fs::rename(&e.dst, &target) {
                Ok(_) => ok += 1,
                Err(_) => match std::fs::copy(&e.dst, &target) {
                    Ok(_) => {
                        let _ = std::fs::remove_file(&e.dst);
                        ok += 1;
                    }
                    Err(_) => skip += 1,
                },
            }
        }
        let _ = append(Op::Undo, b, Path::new(""), Path::new(""));
        return (ok, skip);
    }
    (0, 0)
}

/// 目标已存在时加 (2)/(3)…，绝不覆盖。
pub fn unique(path: &Path) -> PathBuf {
    if !path.exists() {
        return path.to_path_buf();
    }
    let parent = path.parent().map(|p| p.to_path_buf()).unwrap_or_default();
    let stem = path
        .file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_default();
    let ext = path
        .extension()
        .map(|e| format!(".{}", e.to_string_lossy()))
        .unwrap_or_default();
    let mut n = 2;
    loop {
        let cand = parent.join(format!("{} ({}){}", stem, n, ext));
        if !cand.exists() {
            return cand;
        }
        n += 1;
        if n > 999 {
            return parent.join(format!("{}-{}{}", stem, crate::now_stamp(), ext));
        }
    }
}

/// 本次运行使用的批次号：取现有最大批次 + 1。
pub fn next_batch() -> u64 {
    read_all().iter().map(|e| e.batch).max().unwrap_or(0) + 1
}
