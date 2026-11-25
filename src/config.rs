//! 配置读写（自写 INI，零依赖）。
//!
//! 用 INI 而不是 JSON 的理由：单行 `键=值` 可以增量修补，用户能直接手改，
//! 某一行写坏只丢一个字段，不会整份配置解析失败。

use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;

use crate::i18n::Lang;
use crate::winapi::{self, RECT};

const CONFIG_NAME: &str = "config.ini";
const JOURNAL_NAME: &str = "journal.log";

// ------------------------------------------------------------------ 枚举

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum SortMode {
    Name,
    Ext,
    Mtime,
    Manual,
}

impl SortMode {
    pub fn as_str(self) -> &'static str {
        match self {
            SortMode::Name => "name",
            SortMode::Ext => "ext",
            SortMode::Mtime => "mtime",
            SortMode::Manual => "manual",
        }
    }
    pub fn parse(s: &str) -> Self {
        match s.trim().to_ascii_lowercase().as_str() {
            "ext" => SortMode::Ext,
            "mtime" => SortMode::Mtime,
            "manual" => SortMode::Manual,
            _ => SortMode::Name,
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum IconPolicy {
    /// 我们负责维持隐藏状态
    Auto,
    /// 只显示状态，不跟用户抢
    Respect,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum DragMode {
    /// 用 WM_DROPFILES：稳，已实测通过，但没有拖动中的悬停高亮
    DropFiles,
    /// 用 OLE IDropTarget：有悬停高亮，但依赖注册成功
    Ole,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum DockEdge {
    Left,
    Right,
    Top,
}

impl DockEdge {
    pub fn parse(s: &str) -> Self {
        match s.trim().to_ascii_lowercase().as_str() {
            "left" => DockEdge::Left,
            "top" => DockEdge::Top,
            _ => DockEdge::Right,
        }
    }
    pub fn as_str(self) -> &'static str {
        match self {
            DockEdge::Left => "left",
            DockEdge::Right => "right",
            DockEdge::Top => "top",
        }
    }
}

// ------------------------------------------------------------------ 配置结构

#[derive(Clone)]
pub struct GlobalCfg {
    pub hide_icons: bool,
    pub icon_policy: IconPolicy,
    pub autostart: bool,
    pub dry_run: bool,
    pub first_run_done: bool,
    pub dock_mode: bool,
    pub dock_edge: DockEdge,
    pub hotkey: String,
    pub classify_lnk: bool,
    pub opacity: f32,
    pub last_batch: u32,
    pub drag_mode: DragMode,
    /// 连桌面上的文件夹也收进围栏
    pub include_dirs: bool,
    /// 界面语言
    pub lang: Lang,
}

impl Default for GlobalCfg {
    fn default() -> Self {
        Self {
            hide_icons: true,
            icon_policy: IconPolicy::Auto,
            autostart: false,
            dry_run: false,
            first_run_done: false,
            dock_mode: false,
            dock_edge: DockEdge::Right,
            hotkey: "Ctrl+Alt+F".to_string(),
            classify_lnk: false,
            opacity: 0.78,
            last_batch: 0,
            drag_mode: DragMode::DropFiles,
            include_dirs: true,
            lang: Lang::Zh,
        }
    }
}

#[derive(Clone)]
pub struct FenceCfg {
    pub id: u32,
    pub name: String,
    pub dir: PathBuf,
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
    pub h_expanded: i32,
    pub cols: i32,
    pub opacity: f32,
    pub collapsed: bool,
    pub sort: SortMode,
    pub locked: bool,
    pub snap: bool,
    pub color: u32,
    pub rules: Vec<String>,
}

impl FenceCfg {
    pub fn has_rule(&self, ext: &str) -> bool {
        let e = ext.trim_start_matches('.').to_ascii_lowercase();
        self.rules.iter().any(|r| r.eq_ignore_ascii_case(&e))
    }
}

pub struct Config {
    pub global: GlobalCfg,
    pub fences: Vec<FenceCfg>,
}

// ------------------------------------------------------------------ 路径

/// 仅调试版可用的沙箱根目录：把「桌面」「围栏根目录」「配置目录」全部重定向，
/// 保证自测永远不碰真实的桌面和 %APPDATA%。发布版里这个值永远是 None。
static mut TEST_ROOT: Option<PathBuf> = None;

#[cfg(debug_assertions)]
pub fn set_test_root(p: PathBuf) {
    // 必须统一成反斜杠：SHGetFileInfoW 对 "E:/a\b" 这种混合分隔符返回 0，
    // 图标会全部拿不到（实测踩过）。
    let norm = PathBuf::from(p.to_string_lossy().replace('/', "\\"));
    unsafe {
        TEST_ROOT = Some(norm);
    }
}

fn test_root() -> Option<&'static PathBuf> {
    unsafe { (*std::ptr::addr_of!(TEST_ROOT)).as_ref() }
}

pub fn data_dir() -> PathBuf {
    if let Some(r) = test_root() {
        return r.join(".config");
    }
    std::env::var_os("APPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."))
        .join("DesktopFences")
}

pub fn config_path() -> PathBuf {
    data_dir().join(CONFIG_NAME)
}

pub fn journal_path() -> PathBuf {
    data_dir().join(JOURNAL_NAME)
}

/// 围栏绑定的根目录：`%USERPROFILE%\DesktopFences`
pub fn fences_root() -> PathBuf {
    if let Some(r) = test_root() {
        return r.join("DesktopFences");
    }
    std::env::var_os("USERPROFILE")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."))
        .join("DesktopFences")
}

/// 公共桌面：很多安装程序把快捷方式放这里，全用户可见。
pub fn public_desktop_dir() -> PathBuf {
    if let Some(r) = test_root() {
        return r.join("PublicDesktop");
    }
    std::env::var_os("PUBLIC")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(r"C://Users//Public"))
        .join("Desktop")
}

/// 桌面目录（M3 会换成 SHGetKnownFolderPath，兼容 OneDrive 重定向）。
pub fn desktop_dir() -> PathBuf {
    if let Some(r) = test_root() {
        return r.join("Desktop");
    }
    std::env::var_os("USERPROFILE")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."))
        .join("Desktop")
}

// ------------------------------------------------------------------ 默认配置

// 默认只建一个围栏，所有已知类型都收进它；快捷方式与未知类型不动。
const DEFAULT_RULES: &[(&str, &str)] = &[(
    "桌面收纳",
    "doc,docx,xls,xlsx,ppt,pptx,pdf,txt,md,rtf,csv,\
     png,jpg,jpeg,gif,bmp,webp,svg,ico,tif,tiff,\
     mp4,mkv,avi,mov,wmv,flv,webm,\
     mp3,wav,flac,m4a,aac,ogg,\
     zip,rar,7z,tar,gz,bz2,\
     exe,msi,msix,appx,iso,jar,war,\
     c,cpp,cc,cxx,h,hpp,cs,java,py,js,ts,jsx,tsx,go,rs,php,rb,swift,kt,sql,\
     json,xml,yaml,yml,html,htm,css,sh,bat,ps1",
)];

/// 首次运行：只建一个围栏，摆在主屏右侧，6 列 5 行。
///
/// 注意：配置里存的是**物理像素**，所以默认尺寸必须按 DPI 放大，
/// 否则 150% 缩放下列会挤不下。
pub fn defaults(work: RECT, dpi: i32) -> Config {
    let s = |v: i32| ((v as f32) * dpi as f32 / 96.0).round() as i32;
    let panel_w = s(600);
    let panel_h = s(560);
    let cols = 6;
    let x = work.right - panel_w - s(16);
    let y = work.top + s(16);

    let mut fences = Vec::new();
    for (i, (name, rules)) in DEFAULT_RULES.iter().enumerate() {
        fences.push(FenceCfg {
            id: i as u32,
            name: (*name).to_string(),
            dir: fences_root().join(name),
            x,
            y: y + i as i32 * s(12),
            w: panel_w,
            h: panel_h,
            h_expanded: panel_h,
            cols,
            opacity: 0.82,
            collapsed: false,
            sort: SortMode::Mtime,
            locked: false,
            snap: false,
            color: 0x2F3442,
            rules: rules
                .split(',')
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect(),
        });
    }

    Config { global: GlobalCfg::default(), fences }
}

// ------------------------------------------------------------------ 读写

fn parse_ini(text: &str) -> BTreeMap<String, BTreeMap<String, String>> {
    let mut out: BTreeMap<String, BTreeMap<String, String>> = BTreeMap::new();
    let mut cur = String::new();
    for line in text.lines() {
        let l = line.trim();
        if l.is_empty() || l.starts_with(';') || l.starts_with('#') {
            continue;
        }
        if l.starts_with('[') {
            if let Some(end) = l.find(']') {
                cur = l[1..end].trim().to_ascii_lowercase();
                out.entry(cur.clone()).or_default();
                continue;
            }
        }
        if let Some((k, v)) = l.split_once('=') {
            let key = k.trim().to_ascii_lowercase();
            let val = v.trim().to_string();
            if !key.is_empty() {
                out.entry(cur.clone()).or_default().insert(key, val);
            }
        }
    }
    out
}

fn b(m: &BTreeMap<String, String>, k: &str, d: bool) -> bool {
    m.get(k).map(|v| v == "1" || v.eq_ignore_ascii_case("true")).unwrap_or(d)
}
fn i32v(m: &BTreeMap<String, String>, k: &str, d: i32) -> i32 {
    m.get(k).and_then(|v| v.parse().ok()).unwrap_or(d)
}
fn f32v(m: &BTreeMap<String, String>, k: &str, d: f32) -> f32 {
    m.get(k).and_then(|v| v.parse().ok()).unwrap_or(d)
}
fn s(m: &BTreeMap<String, String>, k: &str, d: &str) -> String {
    m.get(k).cloned().unwrap_or_else(|| d.to_string())
}

fn parse_color(v: &str, d: u32) -> u32 {
    let t = v.trim().trim_start_matches('#').trim_start_matches("0x");
    u32::from_str_radix(t, 16).unwrap_or(d)
}

impl Config {
    pub fn load() -> Option<Config> {
        let text = fs::read_to_string(config_path()).ok()?;
        let ini = parse_ini(&text);
        let g = ini.get("global").cloned().unwrap_or_default();

        let global = GlobalCfg {
            hide_icons: b(&g, "hide_desktop_icons", true),
            icon_policy: if s(&g, "icon_policy", "auto").eq_ignore_ascii_case("respect") {
                IconPolicy::Respect
            } else {
                IconPolicy::Auto
            },
            autostart: b(&g, "autostart", false),
            dry_run: b(&g, "dry_run", false),
            first_run_done: b(&g, "first_run_done", true),
            dock_mode: b(&g, "dock_mode", false),
            dock_edge: DockEdge::parse(&s(&g, "dock_edge", "right")),
            hotkey: s(&g, "hotkey", "Ctrl+Alt+F"),
            classify_lnk: b(&g, "classify_lnk", false),
            opacity: f32v(&g, "opacity", 0.78).clamp(0.15, 1.0),
            last_batch: g.get("last_batch").and_then(|v| v.parse().ok()).unwrap_or(0),
            drag_mode: if s(&g, "drag_mode", "dropfiles").eq_ignore_ascii_case("ole") {
                DragMode::Ole
            } else {
                DragMode::DropFiles
            },
            include_dirs: b(&g, "include_dirs", true),
            lang: Lang::from_str(&s(&g, "lang", "zh")),
        };

        let mut fences = Vec::new();
        for (section, m) in ini.iter() {
            if !section.starts_with("fence.") {
                continue;
            }
            let id: u32 = section[6..].parse().unwrap_or(fences.len() as u32);
            let name = s(m, "name", "围栏");
            // 跨电脑可运行：配置里存的是绝对路径，换台机器就失效。
            // 目录不存在时按围栏名在当前用户的根目录下重建。
            let dir = match m.get("dir") {
                Some(d) => {
                    let p = PathBuf::from(d.replace('/', "\\"));
                    if p.exists() {
                        p
                    } else {
                        fences_root().join(&name)
                    }
                }
                None => fences_root().join(&name),
            };
            let h = i32v(m, "h", 252);
            let h_expanded = i32v(m, "h_expanded", h);
            let rules = s(m, "rules", "")
                .split(',')
                .map(|x| x.trim().to_string())
                .filter(|x| !x.is_empty())
                .collect();
            fences.push(FenceCfg {
                id,
                name,
                dir,
                x: i32v(m, "x", -1),
                y: i32v(m, "y", 40),
                w: i32v(m, "w", 400),
                h,
                h_expanded,
                cols: i32v(m, "cols", 4).clamp(1, 12),
                opacity: f32v(m, "opacity", global.opacity).clamp(0.15, 1.0),
                collapsed: b(m, "collapsed", false),
                sort: SortMode::parse(&s(m, "sort", "name")),
                locked: b(m, "locked", false),
                snap: b(m, "snap", false),
                color: parse_color(&s(m, "color", "#2F3442"), 0x2F3442),
                rules,
            });
        }
        fences.sort_by_key(|f| f.id);
        if fences.is_empty() {
            return None;
        }
        Some(Config { global, fences })
    }

    pub fn save(&self) -> std::io::Result<()> {
        let dir = data_dir();
        fs::create_dir_all(&dir)?;
        let mut out = String::new();
        out.push_str("; 桌面围栏配置 —— 可以手改，改完重启程序生效\n[global]\n");
        out.push_str("version=1\n");
        out.push_str(&format!("hide_desktop_icons={}\n", self.global.hide_icons as u8));
        out.push_str(&format!(
            "icon_policy={}\n",
            match self.global.icon_policy {
                IconPolicy::Auto => "auto",
                IconPolicy::Respect => "respect",
            }
        ));
        out.push_str(&format!("autostart={}\n", self.global.autostart as u8));
        out.push_str(&format!("dry_run={}\n", self.global.dry_run as u8));
        out.push_str(&format!("first_run_done={}\n", self.global.first_run_done as u8));
        out.push_str(&format!("dock_mode={}\n", self.global.dock_mode as u8));
        out.push_str(&format!("dock_edge={}\n", self.global.dock_edge.as_str()));
        out.push_str(&format!("hotkey={}\n", self.global.hotkey));
        out.push_str(&format!("classify_lnk={}\n", self.global.classify_lnk as u8));
        out.push_str(&format!("opacity={:.2}\n", self.global.opacity));
        out.push_str(&format!("last_batch={}\n", self.global.last_batch));
        out.push_str(&format!("include_dirs={}\n", self.global.include_dirs as u8));
        out.push_str(&format!("lang={}\n", self.global.lang.as_str()));
        out.push_str(&format!(
            "drag_mode={}\n",
            match self.global.drag_mode {
                DragMode::Ole => "ole",
                DragMode::DropFiles => "dropfiles",
            }
        ));

        for f in &self.fences {
            out.push_str(&format!("\n[fence.{}]\n", f.id));
            out.push_str(&format!("name={}\n", f.name));
            out.push_str(&format!("dir={}\n", f.dir.display()));
            out.push_str(&format!("x={}\ny={}\nw={}\nh={}\n", f.x, f.y, f.w, f.h));
            out.push_str(&format!("h_expanded={}\n", f.h_expanded));
            out.push_str(&format!("cols={}\n", f.cols));
            out.push_str(&format!("opacity={:.2}\n", f.opacity));
            out.push_str(&format!("collapsed={}\n", f.collapsed as u8));
            out.push_str(&format!("sort={}\n", f.sort.as_str()));
            out.push_str(&format!("locked={}\n", f.locked as u8));
            out.push_str(&format!("snap={}\n", f.snap as u8));
            out.push_str(&format!("color=#{:06X}\n", f.color));
            out.push_str(&format!("rules={}\n", f.rules.join(",")));
        }

        let dst = config_path();
        let tmp = dir.join("config.ini.tmp");
        fs::write(&tmp, out.as_bytes())?;
        let (tw, dw) = (winapi::w(&tmp.to_string_lossy()), winapi::w(&dst.to_string_lossy()));
        let ok = unsafe {
            winapi::MoveFileExW(tw.as_ptr(), dw.as_ptr(), winapi::MOVEFILE_REPLACE_EXISTING)
        };
        if ok == 0 {
            let _ = fs::copy(&tmp, &dst);
            let _ = fs::remove_file(&tmp);
        }
        Ok(())
    }

    /// 首次运行：建围栏目录并落盘。
    pub fn init_dirs(&self) {
        let _ = fs::create_dir_all(fences_root());
        for f in &self.fences {
            let _ = fs::create_dir_all(&f.dir);
        }
    }

    pub fn fence_by_rule(&self, ext: &str) -> Option<&FenceCfg> {
        let e = ext.trim_start_matches('.').to_ascii_lowercase();
        if e.is_empty() {
            return None;
        }
        if (e == "lnk" || e == "url") && !self.global.classify_lnk {
            return None;
        }
        self.fences.iter().find(|f| f.has_rule(&e))
    }
}
