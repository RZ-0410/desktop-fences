//! 自动归类：盯着桌面目录，等文件「真的写完」再搬进围栏。
//!
//! 判断「写完」不能靠固定延时。一个 2GB 的下载、一次 Photoshop 保存，
//! 完全可能在两次采样之间大小不变但仍在写。这里用三层判定：
//!
//! 1. **事件静默**：`ReadDirectoryChangesW` 只要还在写就会持续给 SIZE/LAST_WRITE 事件，
//!    连续 1200ms 没有新事件才算安静下来；
//! 2. **写句柄探测**：用「拒绝其他写者」的共享模式去 `CreateFileW`，如果返回
//!    `ERROR_SHARING_VIOLATION`，说明还有进程以写方式打开着（记事本、Office、下载器都算），
//!    这一条才是真正的「建立完成」信号；
//! 3. **超时兜底**：一直占用超过 60 秒就放弃，文件留在桌面，绝不硬搬。
//!
//! 另外：下载中的 `.crdownload`/`.part`、`~$` 临时文件、隐藏/系统属性、快捷方式一律不碰。

use std::collections::HashMap;
use std::ffi::c_void;
use std::mem::zeroed;
use std::path::{Path, PathBuf};
use std::ptr::null_mut;
use std::time::{Duration, Instant};

use crate::journal::{self, Op};
use crate::winapi::*;

/// 连续多久没有新事件才算写完
const QUIET_MS: u128 = 1200;
/// 一直被占用就把等待上限卡在这里
const MAX_WAIT_MS: u128 = 60_000;
/// 单次等待切片，用来在没事件时也推进重试
const TICK_MS: u32 = 300;

pub struct Watcher {
    stop: HANDLE,
}

impl Watcher {
    pub fn stop(&self) {
        unsafe {
            SetEvent(self.stop);
        }
    }
}

impl Drop for Watcher {
    fn drop(&mut self) {
        self.stop();
        unsafe {
            CloseHandle(self.stop);
        }
    }
}

struct Pending {
    first_seen: Instant,
    last_event: Instant,
    last_size: u64,
}

/// 启动监听线程。`rules` 是 (围栏目录, 扩展名列表) 的快照。
pub fn start(
    desktop: PathBuf,
    rules: Vec<(PathBuf, Vec<String>)>,
    classify_lnk: bool,
    include_dirs: bool,
    default_dir: PathBuf,
    dry_run: bool,
    hctrl: HWND,
    batch: u64,
) -> Option<Watcher> {
    if rules.is_empty() {
        return None;
    }
    unsafe {
        let stop = CreateEventW(null_mut(), 1, 0, null_mut());
        if stop.is_null() {
            return None;
        }
        let stop_for_thread = stop as usize;
        let hctrl_for_thread = hctrl as usize;
        std::thread::spawn(move || {
            worker(
                desktop,
                rules,
                classify_lnk,
                include_dirs,
                default_dir,
                dry_run,
                hctrl_for_thread as HWND,
                batch,
                stop_for_thread as HANDLE,
            );
        });
        Some(Watcher { stop })
    }
}

unsafe fn worker(
    desktop: PathBuf,
    rules: Vec<(PathBuf, Vec<String>)>,
    classify_lnk: bool,
    include_dirs: bool,
    default_dir: PathBuf,
    dry_run: bool,
    hctrl: HWND,
    batch: u64,
    stop: HANDLE,
) {
    let dw = w(&desktop.to_string_lossy());
    let h = CreateFileW(
        dw.as_ptr(),
        FILE_LIST_DIRECTORY,
        FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE,
        null_mut(),
        OPEN_EXISTING,
        FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OVERLAPPED,
        null_mut(),
    );
    if h == INVALID_HANDLE_VALUE || h.is_null() {
        crate::dlog("WATCHER 打开桌面目录失败");
        return;
    }
    let ev = CreateEventW(null_mut(), 1, 0, null_mut());
    if ev.is_null() {
        CloseHandle(h);
        return;
    }

    let filter =
        FILE_NOTIFY_CHANGE_FILE_NAME | FILE_NOTIFY_CHANGE_SIZE | FILE_NOTIFY_CHANGE_LAST_WRITE;
    let mut pending: HashMap<String, Pending> = HashMap::new();

    'outer: loop {
        // 发起一次异步读取，然后在这个操作上等
        let mut ov: OVERLAPPED = zeroed();
        ov.hEvent = ev;
        let mut buf = vec![0u8; 64 * 1024];
        let mut bytes: DWORD = 0;
        if ReadDirectoryChangesW(
            h,
            buf.as_mut_ptr() as *mut c_void,
            buf.len() as u32,
            0,
            filter,
            &mut bytes,
            &mut ov,
            null_mut(),
        ) == 0
        {
            crate::dlog("WATCHER ReadDirectoryChangesW 失败");
            break;
        }

        loop {
            let handles = [stop, ev];
            let r = WaitForMultipleObjects(2, handles.as_ptr(), 0, TICK_MS);
            if r == 0 {
                CancelIoEx(h, &mut ov);
                break 'outer;
            }
            if r == 1 {
                let mut got: DWORD = 0;
                if GetOverlappedResult(h, &mut ov, &mut got, 0) != 0 {
                    let now = Instant::now();
                    for (action, name) in parse_changes(&buf, got) {
                        if action == FILE_ACTION_ADDED
                            || action == FILE_ACTION_RENAMED_NEW_NAME
                            || action == FILE_ACTION_MODIFIED
                        {
                            let size = std::fs::metadata(desktop.join(&name))
                                .map(|m| m.len())
                                .unwrap_or(0);
                            pending
                                .entry(name)
                                .and_modify(|p| {
                                    p.last_event = now;
                                    p.last_size = size;
                                })
                                .or_insert(Pending {
                                    first_seen: now,
                                    last_event: now,
                                    last_size: size,
                                });
                        }
                    }
                }
                break;
            }
            // 超时：没有新事件，但仍然要推进重试
            if process_due(
                &mut pending,
                &desktop,
                &rules,
                classify_lnk,
                include_dirs,
                &default_dir,
                dry_run,
                batch,
                hctrl,
            ) {
                // 有文件落位，立刻刷新面板
            }
        }

        if process_due(
            &mut pending,
            &desktop,
            &rules,
            classify_lnk,
            include_dirs,
            &default_dir,
            dry_run,
            batch,
            hctrl,
        ) {
            if !hctrl.is_null() {
                PostMessageW(hctrl, crate::WM_APP_FS, 0, 0);
            }
        }
    }

    CloseHandle(ev);
    CloseHandle(h);
}

/// 处理所有「安静够久」的候选。返回是否有文件被搬走。
fn process_due(
    pending: &mut HashMap<String, Pending>,
    desktop: &Path,
    rules: &[(PathBuf, Vec<String>)],
    classify_lnk: bool,
    include_dirs: bool,
    default_dir: &Path,
    dry_run: bool,
    batch: u64,
    hctrl: HWND,
) -> bool {
    let now = Instant::now();
    let due: Vec<String> = pending
        .iter()
        .filter(|(k, p)| now.duration_since(p.last_event).as_millis() >= quiet_for(k))
        .map(|(k, _)| k.clone())
        .collect();
    if due.is_empty() {
        return false;
    }

    let mut moved_any = false;
    for name in due {
        if !acceptable(&name) {
            pending.remove(&name);
            continue;
        }
        let src = desktop.join(&name);
        let meta = match std::fs::metadata(&src) {
            Ok(m) => m,
            Err(_) => {
                // 文件被删/改名了，放弃
                pending.remove(&name);
                continue;
            }
        };
        let is_dir = meta.is_dir();
        if is_dir && !include_dirs {
            pending.remove(&name);
            continue;
        }
        if has_bad_attr(&src) {
            pending.remove(&name);
            continue;
        }

        // 关键一步：还有写句柄就继续等，别搬一个正在写的文件。目录不做这个检查。
        if !is_dir && writer_active(&src) {
            let p = pending.get_mut(&name).unwrap();
            if now.duration_since(p.first_seen).as_millis() > MAX_WAIT_MS {
                crate::dlog(&format!("WATCHER 占用超时，放弃 {}", src.display()));
                pending.remove(&name);
            } else {
                // 退避 1.2 秒后再试
                p.last_event = now;
            }
            continue;
        }

        let target_dir = if is_dir {
            // 文件夹没有扩展名，统一收进第一个围栏
            default_dir.to_path_buf()
        } else {
            let ext = src
                .extension()
                .map(|e| e.to_string_lossy().to_ascii_lowercase())
                .unwrap_or_default();
            match target_for(&ext, rules, classify_lnk, default_dir) {
                Some(d) => d,
                None => {
                    pending.remove(&name);
                    continue;
                }
            }
        };

        if dry_run {
            let _ = journal::append(Op::Move, batch, &src, &target_dir.join(&name));
            crate::dlog(&format!("WATCHER [dry-run] 将移动 {}", src.display()));
            pending.remove(&name);
            continue;
        }

        let _ = std::fs::create_dir_all(&target_dir);
        let target = journal::unique(&target_dir.join(&name));
        let moved = match std::fs::rename(&src, &target) {
            Ok(_) => true,
            Err(_) => match std::fs::copy(&src, &target) {
                Ok(_) => {
                    let _ = std::fs::remove_file(&src);
                    true
                }
                Err(e) => {
                    crate::dlog(&format!("WATCHER 移动失败 {}：{}", src.display(), e));
                    false
                }
            },
        };
        if moved {
            let _ = journal::append(Op::Move, batch, &src, &target);
            crate::dlog(&format!("WATCHER 归位 {} -> {}", src.display(), target.display()));
            pending.remove(&name);
            moved_any = true;
        } else {
            pending.remove(&name);
        }
    }

    let _ = hctrl;
    moved_any
}

/// 是否还有进程以「写」方式打开着这个文件。
///
/// 用 `FILE_SHARE_READ | FILE_SHARE_DELETE`（**不共享写**）去开：只要别人握着写句柄，
/// 这里就会拿到 `ERROR_SHARING_VIOLATION`，说明文件还在被写。
fn writer_active(path: &Path) -> bool {
    unsafe {
        let pw = w(&path.to_string_lossy());
        let h = CreateFileW(
            pw.as_ptr(),
            GENERIC_READ,
            FILE_SHARE_READ | FILE_SHARE_DELETE,
            null_mut(),
            OPEN_EXISTING,
            FILE_ATTRIBUTE_NORMAL,
            null_mut(),
        );
        if h == INVALID_HANDLE_VALUE {
            let err = GetLastError() as i32;
            return err == ERROR_SHARING_VIOLATION || err == ERROR_LOCK_VIOLATION;
        }
        CloseHandle(h);
        false
    }
}

unsafe fn parse_changes(buf: &[u8], _bytes: u32) -> Vec<(u32, String)> {
    let mut out = Vec::new();
    let mut off = 0usize;
    loop {
        if off + std::mem::size_of::<FILE_NOTIFY_INFORMATION>() > buf.len() {
            break;
        }
        let p = buf.as_ptr().add(off) as *const FILE_NOTIFY_INFORMATION;
        let action = (*p).Action;
        let len = (*p).FileNameLength as usize / 2;
        let name_ptr = (*p).FileName.as_ptr();
        if len > 0 && len < 260 {
            let name = String::from_utf16_lossy(std::slice::from_raw_parts(name_ptr, len));
            out.push((action, name));
        }
        let next = (*p).NextEntryOffset as usize;
        if next == 0 {
            break;
        }
        off += next;
    }
    out
}

/// 明确不碰的东西。
fn acceptable(name: &str) -> bool {
    if name.is_empty() || name.starts_with('.') || name.starts_with("~$") {
        return false;
    }
    let lower = name.to_ascii_lowercase();
    if lower == "desktop.ini" {
        return false;
    }
    for bad in [".tmp", ".crdownload", ".part", ".partial", ".download"] {
        if lower.ends_with(bad) {
            return false;
        }
    }
    true
}

/// 目标围栏：快捷方式只看开关（不受规则表限制），其余看扩展名规则。
fn target_for(
    ext: &str,
    rules: &[(PathBuf, Vec<String>)],
    classify_lnk: bool,
    default_dir: &Path,
) -> Option<PathBuf> {
    if ext.is_empty() {
        return None;
    }
    if ext == "lnk" || ext == "url" {
        return if classify_lnk {
            Some(default_dir.to_path_buf())
        } else {
            None
        };
    }
    rules
        .iter()
        .find(|(_, exts)| exts.iter().any(|e| e.eq_ignore_ascii_case(ext)))
        .map(|(dir, _)| dir.clone())
}

/// 右键「新建 → 文本文档」后用户往往立刻改名，这时文件是关闭的，
/// 按普通静默时间会被提前搬走。给这类系统默认名更长的观察期。
fn quiet_for(name: &str) -> u128 {
    if name.starts_with("新建") || name.starts_with("New ") || name.starts_with("Neues ") {
        QUIET_MS * 4
    } else {
        QUIET_MS
    }
}

/// 隐藏/系统属性过滤（打开前先挡掉，避免无谓的句柄探测）。
fn has_bad_attr(path: &Path) -> bool {
    let pw = w(&path.to_string_lossy());
    let attr = unsafe { GetFileAttributesW(pw.as_ptr()) };
    attr != INVALID_FILE_ATTRIBUTES && (attr & (FILE_ATTRIBUTE_HIDDEN | FILE_ATTRIBUTE_SYSTEM)) != 0
}

// ------------------------------------------------------------------ 一次性整理

/// 扫描桌面，列出「可以被整理」的文件（源文件 → 目标目录）。
/// 只读，不动任何文件；用于弹确认框前先算出数量。
pub fn plan_tidy(
    desktops: &[PathBuf],
    rules: &[(PathBuf, Vec<String>)],
    classify_lnk: bool,
    include_dirs: bool,
    default_dir: &Path,
) -> Vec<(PathBuf, PathBuf)> {
    let mut plan = Vec::new();
    for desktop in desktops {
        plan_dir(desktop, rules, classify_lnk, include_dirs, default_dir, &mut plan);
    }
    plan
}

fn plan_dir(
    desktop: &Path,
    rules: &[(PathBuf, Vec<String>)],
    classify_lnk: bool,
    include_dirs: bool,
    default_dir: &Path,
    plan: &mut Vec<(PathBuf, PathBuf)>,
) {
    let rd = match std::fs::read_dir(desktop) {
        Ok(r) => r,
        Err(_) => return,
    };
    for ent in rd.flatten() {
        let name = ent.file_name().to_string_lossy().to_string();
        if !acceptable(&name) {
            continue;
        }
        let path = ent.path();
        let meta = match ent.metadata() {
            Ok(m) => m,
            Err(_) => continue,
        };
        if has_bad_attr(&path) {
            continue;
        }
        if meta.is_dir() {
            if include_dirs {
                plan.push((path, default_dir.to_path_buf()));
            }
            continue;
        }
        let ext = path
            .extension()
            .map(|e| e.to_string_lossy().to_ascii_lowercase())
            .unwrap_or_default();
        if let Some(dir) = target_for(&ext, rules, classify_lnk, default_dir) {
            plan.push((path, dir));
        }
    }
}

/// 执行整理。正在被占用的文件跳过，不硬搬。返回 (成功, 跳过)。
pub fn apply_plan(plan: Vec<(PathBuf, PathBuf)>, batch: u64, dry_run: bool) -> (usize, usize) {
    let mut ok = 0usize;
    let mut skip = 0usize;
    for (src, dir) in plan {
        if writer_active(&src) {
            crate::dlog(&format!("TIDY 跳过占用中 {}", src.display()));
            skip += 1;
            continue;
        }
        if dry_run {
            let _ = journal::append(Op::Move, batch, &src, &dir.join(src.file_name().unwrap_or_default()));
            ok += 1;
            continue;
        }
        let _ = std::fs::create_dir_all(&dir);
        let target = journal::unique(&dir.join(src.file_name().unwrap_or_default()));
        let moved = match std::fs::rename(&src, &target) {
            Ok(_) => true,
            Err(_) => match std::fs::copy(&src, &target) {
                Ok(_) => {
                    let _ = std::fs::remove_file(&src);
                    true
                }
                Err(_) => false,
            },
        };
        if moved {
            let _ = journal::append(Op::Move, batch, &src, &target);
            ok += 1;
        } else {
            skip += 1;
        }
    }
    (ok, skip)
}
