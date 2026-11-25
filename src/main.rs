//! 桌面围栏 —— 零依赖 Windows 桌面分区收纳工具。
//!
//! 启动顺序有讲究：DPI 感知 → 单实例 → OLE(STA) → 注册窗口类 → 控制器窗口
//! → 读配置 → 建围栏 → 托盘 → 前台事件钩子 → 消息循环。

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
#![allow(non_snake_case)]

mod classify;
mod config;
mod drop;
mod fence_dlg;
mod fence_ui;
mod gfx;
mod i18n;
mod icons;
mod journal;
mod settings_ui;
mod shell;
mod tray;
mod winapi;

use std::mem::size_of;
use std::path::{Path, PathBuf};
use std::ptr::null_mut;

use config::{Config, FenceCfg, IconPolicy, SortMode};
use fence_ui::Fence;
use winapi::*;

pub const WM_APP_TRAY: UINT = WM_APP + 1;
const WM_APP_REZORDER: UINT = WM_APP + 2;
const WM_APP_SHOW: UINT = WM_APP + 3;
pub const WM_APP_FS: UINT = WM_APP + 4;

const TIMER_SYNC: usize = 1;
const CTRL_CLASS: &str = "DFController";

// ------------------------------------------------------------------ 全局状态

pub struct App {
    pub hinst: HINSTANCE,
    pub hctrl: HWND,
    pub cfg: Config,
    pub fences: Vec<Fence>,
    pub icons_hidden_by_us: bool,
    pub tray_ok: bool,
    pub taskbar_created: UINT,
    pub hidden_all: bool,
    pub fullscreen_hidden: bool,
    pub quitting: bool,
    /// 沙箱模式：绝不碰真实桌面图标
    pub sandbox: bool,
    /// 桌面监听线程，退出时 drop 掉
    pub watcher: Option<classify::Watcher>,
}

static mut APP: *mut App = null_mut();

pub fn app() -> &'static mut App {
    unsafe { &mut *APP }
}

/// 秒级时间戳，用于极端重名时兜底命名。
pub fn now_stamp() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// 调试版日志，写当前目录的 debug.log；发布版编译成空操作。
pub fn dlog(msg: &str) {
    #[cfg(debug_assertions)]
    {
        use std::io::Write;
        // 日志写盘也是磁盘 I/O：默认关闭，需要排查时设 DESKFENCE_LOG=1
        static ON: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
        let on = *ON.get_or_init(|| std::env::var_os("DESKFENCE_LOG").is_some());
        if !on {
            return;
        }
        if let Ok(mut f) = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open("debug.log")
        {
            let _ = writeln!(f, "{}", msg);
        }
    }
    #[cfg(not(debug_assertions))]
    {
        let _ = msg;
    }
}

pub fn fence_mut(hwnd: HWND) -> Option<&'static mut Fence> {
    app().fences.iter_mut().find(|f| f.hwnd == hwnd)
}

/// 上次隐藏了桌面图标却没恢复就退出（崩溃/被强杀）时留下的标记。
/// 下次启动先看它，把图标还回去，避免用户对着空桌面找不到东西。
pub fn icon_marker() -> PathBuf {
    config::data_dir().join("icons_hidden.marker")
}

pub fn mark_icons_hidden() {
    let _ = std::fs::write(icon_marker(), format!("{}\n", std::process::id()));
}

pub fn clear_icon_marker() {
    let _ = std::fs::remove_file(icon_marker());
}

pub fn save_config() {
    let a = app();
    for f in &a.fences {
        if let Some(c) = a.cfg.fences.iter_mut().find(|c| c.id == f.cfg.id) {
            *c = f.cfg.clone();
        }
    }
    let _ = a.cfg.save();
}

// ------------------------------------------------------------------ 入口

fn main() {
    unsafe { run() }
}

unsafe fn run() {
    set_dpi_awareness();
    let hinst = GetModuleHandleW(null_mut());

    // 调试版沙箱：--test-root=<dir>，把桌面/配置/围栏目录全重定向
    #[cfg(debug_assertions)]
    {
        for arg in std::env::args().skip(1) {
            if let Some(p) = arg.strip_prefix("--test-root=") {
                config::set_test_root(PathBuf::from(p));
            }
        }
    }
    dlog("RUN start");
    let sandbox = cfg!(debug_assertions)
        && std::env::args().any(|a| a.starts_with("--test-root="));
    if sandbox {
        let _ = std::fs::create_dir_all(config::fences_root());
        let _ = std::fs::create_dir_all(config::desktop_dir());
    }

    // 单实例
    let mtx_name = w("Local\\DesktopFences.SingleInstance");
    // CreateMutexW 新建互斥体时**不会**清零 GetLastError()，必须先清零，
    // 否则上一次调用留下的 ERROR_ALREADY_EXISTS 会被误判成「已有实例」而静默退出。
    SetLastError(0);
    let _mtx = CreateMutexW(null_mut(), 0, mtx_name.as_ptr());
    if GetLastError() as i32 == ERROR_ALREADY_EXISTS {
        dlog("RUN 互斥体已存在，退出");
        let h = FindWindowW(w(CTRL_CLASS).as_ptr(), null_mut());
        if !h.is_null() {
            PostMessageW(h, WM_APP_SHOW, 0, 0);
        }
        return;
    }
    dlog("RUN 互斥体 OK");

    // OLE STA：拖放和 shell 都需要
    let hr = OleInitialize(null_mut());
    let _ = hr; // RPC_E_CHANGED_MODE 说明已经是 STA，继续

    let work = primary_work();
    let startup_dpi = fence_ui::default_dpi();
    let mut cfg = match Config::load() {
        Some(c) => c,
        None => config::defaults(work, startup_dpi),
    };

    if !cfg.global.first_run_done {
        if sandbox {
            cfg.global.first_run_done = true;
            cfg.global.hide_icons = false;
        } else if !first_run_confirm(&mut cfg) {
            return;
        }
    }
    // 上次没恢复图标就退出了？先还回去再说
    if !sandbox && icon_marker().exists() {
        shell::set_icons_visible(true);
        clear_icon_marker();
        dlog("启动时发现残留的图标隐藏标记，已恢复桌面图标");
    }

    dlog("RUN 配置载入 OK");
    i18n::set_lang(cfg.global.lang);
    cfg.global.autostart = tray::autostart_enabled();
    cfg.init_dirs();
    let _ = cfg.save();

    let boxed = Box::new(App {
        hinst,
        hctrl: null_mut(),
        cfg,
        fences: Vec::new(),
        icons_hidden_by_us: false,
        tray_ok: false,
        taskbar_created: RegisterWindowMessageW(w("TaskbarCreated").as_ptr()),
        hidden_all: false,
        fullscreen_hidden: false,
        quitting: false,
        sandbox,
        watcher: None,
    });
    APP = Box::into_raw(boxed);

    // 调试版：--drop-test=<文件> 直接跑一遍搬运逻辑（不经过 OLE），用于自动化验证
    #[cfg(debug_assertions)]
    if let Some(arg) = std::env::args().find(|a| a.starts_with("--drop-test=")) {
        let src = PathBuf::from(&arg["--drop-test=".len()..]);
        let dir = app().cfg.fences[0].dir.clone();
        let _ = std::fs::create_dir_all(&dir);
        match drop::transfer(&src, &dir, false) {
            Ok(p) => {
                let batch = journal::next_batch();
                let _ = journal::append(journal::Op::Move, batch, &src, &p);
                dlog(&format!("DROP-TEST ok -> {}", p.display()));
            }
            Err(e) => dlog(&format!("DROP-TEST fail: {}", e)),
        }
        return;
    }

    // 调试版：--origin-test=<文件> 查它当初从哪儿搬来的
    #[cfg(debug_assertions)]
    if let Some(arg) = std::env::args().find(|a| a.starts_with("--origin-test=")) {
        let p = PathBuf::from(&arg["--origin-test=".len()..]);
        match journal::origin_of(&p) {
            Some(o) => dlog(&format!("ORIGIN-TEST {} -> {}", p.display(), o.display())),
            None => dlog(&format!("ORIGIN-TEST {} -> (无记录)", p.display())),
        }
        return;
    }

    // 调试版：--arrange-test 只排列桌面图标
    #[cfg(debug_assertions)]
    if std::env::args().any(|a| a == "--arrange-test") {
        let ok = shell::arrange_desktop_icons();
        dlog(&format!("ARRANGE-TEST ok={}", ok));
        return;
    }

    // 调试版：--tidy-plan 只统计会移动哪些，不动文件
    #[cfg(debug_assertions)]
    if std::env::args().any(|a| a == "--tidy-plan") {
        let rules: Vec<(PathBuf, Vec<String>)> = app()
            .cfg
            .fences
            .iter()
            .map(|f| (f.dir.clone(), f.rules.clone()))
            .collect();
        let sources: Vec<PathBuf> = [config::desktop_dir(), config::public_desktop_dir()]
            .into_iter()
            .filter(|d| d.exists())
            .collect();
        let default_dir = app()
            .cfg
            .fences
            .first()
            .map(|f| f.dir.clone())
            .unwrap_or_else(config::fences_root);
        let plan = classify::plan_tidy(
            &sources,
            &rules,
            app().cfg.global.classify_lnk,
            app().cfg.global.include_dirs,
            &default_dir,
        );
        let mut user = 0;
        let mut public = 0;
        for (p, _) in &plan {
            if p.to_string_lossy().contains("\\Public\\") {
                public += 1;
            } else {
                user += 1;
            }
        }
        dlog(&format!("TIDY-PLAN total={} user={} public={}", plan.len(), user, public));
        return;
    }

    // 调试版：--tidy-test 直接整理一次（不弹确认框）
    #[cfg(debug_assertions)]
    if std::env::args().any(|a| a == "--tidy-test") {
        let rules: Vec<(PathBuf, Vec<String>)> = app()
            .cfg
            .fences
            .iter()
            .map(|f| (f.dir.clone(), f.rules.clone()))
            .collect();
        let sources: Vec<PathBuf> = [config::desktop_dir(), config::public_desktop_dir()]
            .into_iter()
            .filter(|d| d.exists())
            .collect();
        let dirs = app().cfg.global.include_dirs;
        let default_dir = app()
            .cfg
            .fences
            .first()
            .map(|f| f.dir.clone())
            .unwrap_or_else(config::fences_root);
        let plan = classify::plan_tidy(
            &sources,
            &rules,
            app().cfg.global.classify_lnk,
            dirs,
            &default_dir,
        );
        let n = plan.len();
        let batch = journal::next_batch();
        let (ok, skip) = classify::apply_plan(plan, batch, app().cfg.global.dry_run);
        dlog(&format!("TIDY-TEST planned={} ok={} skip={}", n, ok, skip));
        return;
    }

    // 调试版：--undo-test 直接走一遍「撤销上次整理」
    #[cfg(debug_assertions)]
    if std::env::args().any(|a| a == "--undo-test") {
        let (ok, skip) = journal::undo_last();
        dlog(&format!("UNDO-TEST ok={} skip={}", ok, skip));
        return;
    }

    dlog("RUN 开始注册窗口类");
    if !register_classes(hinst) {
        dlog("RUN 注册窗口类失败");
        MessageBoxW(
            null_mut(),
            w(i18n::t("class_fail")).as_ptr(),
            w("桌面围栏").as_ptr(),
            MB_OK | MB_ICONWARNING,
        );
        return;
    }

    let hctrl = create_controller(hinst);
    if hctrl.is_null() {
        MessageBoxW(null_mut(), w(i18n::t("ctrl_fail")).as_ptr(), w("桌面围栏").as_ptr(), MB_OK | MB_ICONWARNING);
        return;
    }
    app().hctrl = hctrl;
    dlog("RUN 控制器窗口 OK");

    // 围栏窗口
    let dpi = fence_ui::default_dpi();
    let mut failed = 0;
    for cf in app().cfg.fences.clone() {
        match fence_ui::create(hinst, cf, dpi) {
            Some(f) => app().fences.push(f),
            None => failed += 1,
        }
    }
    if app().fences.is_empty() {
        MessageBoxW(
            null_mut(),
            w(i18n::t("no_fence")).as_ptr(),
            w("桌面围栏").as_ptr(),
            MB_OK | MB_ICONWARNING,
        );
        return;
    }

    for f in &app().fences {
        fence_ui::reorder(f.hwnd);
    }

    // 托盘
    app().tray_ok = tray::add(hctrl, "桌面围栏");
    if !app().tray_ok {
        MessageBoxW(
            null_mut(),
            w(i18n::t("tray_fail")).as_ptr(),
            w("桌面围栏").as_ptr(),
            MB_OK | MB_ICONWARNING,
        );
    }

    // 桌面图标
    if app().cfg.global.hide_icons && !app().sandbox {
        let ok = shell::set_icons_visible(false);
        app().icons_hidden_by_us = ok;
        if ok {
            mark_icons_hidden();
        }
    }

    // 自动归类监听
    start_watcher();

    SetWinEventHook(
        EVENT_SYSTEM_FOREGROUND,
        EVENT_SYSTEM_FOREGROUND,
        null_mut(),
        Some(win_event_proc),
        0,
        0,
        WINEVENT_OUTOFCONTEXT | WINEVENT_SKIPOWNPROCESS,
    );

    dlog("RUN 进入消息循环");
    // 调试版：--props-test 打开第一个围栏的属性对话框，便于自动化验证
    #[cfg(debug_assertions)]
    if std::env::args().any(|a| a == "--props-test") {
        edit_fence_props(app().fences.first().map(|f| f.hwnd).unwrap_or(std::ptr::null_mut()));
        return;
    }

    // 调试版：--delete-test 直接删掉第一个围栏（文件放回桌面），跳过对话框
    #[cfg(debug_assertions)]
    if std::env::args().any(|a| a == "--delete-test") {
        delete_fence_do(0, true);
        return;
    }

    // 调试版：--settings 启动后自动打开设置窗口，便于自动化验证
    #[cfg(debug_assertions)]
    if std::env::args().any(|a| a == "--settings") {
        settings_ui::open(hinst, hctrl);
    }

    SetTimer(hctrl, TIMER_SYNC, 3000, null_mut());

    let mut msg: MSG = std::mem::zeroed();
    while GetMessageW(&mut msg, null_mut(), 0, 0) > 0 {
        TranslateMessage(&msg);
        DispatchMessageW(&msg);
    }

    cleanup();
    OleUninitialize();
    let _ = failed;
}

unsafe fn cleanup() {
    let a = app();
    a.watcher = None;
    if a.icons_hidden_by_us {
        shell::set_icons_visible(true);
        clear_icon_marker();
    }
    if a.tray_ok {
        tray::remove(a.hctrl);
    }
    for f in &mut a.fences {
        DestroyWindow(f.hwnd);
    }
    a.fences.clear();
    if !a.hctrl.is_null() {
        DestroyWindow(a.hctrl);
    }
    save_config();
}

// ------------------------------------------------------------------ 首次运行

unsafe fn first_run_confirm(cfg: &mut Config) -> bool {
    let body = i18n::t("first_run_body");
    let r = MessageBoxW(
        null_mut(),
        w(body).as_ptr(),
        w("桌面围栏 · 首次运行").as_ptr(),
        MB_YESNO | MB_ICONINFORMATION | MB_SETFOREGROUND,
    );
    if r != IDYES {
        return false;
    }
    let r2 = MessageBoxW(
        null_mut(),
        w(i18n::t("first_run_hide")).as_ptr(),
        w("桌面围栏").as_ptr(),
        MB_YESNO | MB_ICONQUESTION | MB_SETFOREGROUND,
    );
    cfg.global.hide_icons = r2 == IDYES;
    cfg.global.first_run_done = true;
    true
}

// ------------------------------------------------------------------ 窗口类

unsafe fn register_classes(hinst: HINSTANCE) -> bool {
    let cls = w(CTRL_CLASS);
    let wc = WNDCLASSEXW {
        cbSize: size_of::<WNDCLASSEXW>() as u32,
        style: 0,
        lpfnWndProc: Some(ctrl_proc),
        cbClsExtra: 0,
        cbWndExtra: 0,
        hInstance: hinst,
        hIcon: LoadIconW(null_mut(), IDI_APPLICATION),
        hCursor: LoadCursorW(null_mut(), IDC_ARROW),
        hbrBackground: null_mut(),
        lpszMenuName: null_mut(),
        lpszClassName: cls.as_ptr(),
        hIconSm: LoadIconW(null_mut(), IDI_APPLICATION),
    };
    if RegisterClassExW(&wc) == 0 {
        return false;
    }
    fence_ui::register(hinst)
}

unsafe fn create_controller(hinst: HINSTANCE) -> HWND {
    let cls = w(CTRL_CLASS);
    CreateWindowExW(
        0,
        cls.as_ptr(),
        w("桌面围栏").as_ptr(),
        WS_POPUP,
        0,
        0,
        0,
        0,
        null_mut(),
        null_mut(),
        hinst,
        null_mut(),
    )
}

// ------------------------------------------------------------------ 控制器窗口

unsafe extern "system" fn ctrl_proc(hwnd: HWND, msg: UINT, wp: WPARAM, lp: LPARAM) -> LRESULT {
    if APP.is_null() {
        return DefWindowProcW(hwnd, msg, wp, lp);
    }
    if msg == app().taskbar_created && msg != 0 {
        // Explorer 重启：句柄全失效，一律重新取
        app().tray_ok = tray::add(hwnd, "桌面围栏");
        for f in &app().fences {
            fence_ui::reorder(f.hwnd);
        }
        if app().cfg.global.hide_icons {
            shell::set_icons_visible(false);
        }
        return 0;
    }

    match msg {
        WM_APP_TRAY => {
            let ev = (lp & 0xffff) as u32;
            if ev == NIN_SELECT || ev == NIN_KEYSELECT || ev == WM_LBUTTONUP {
                toggle_all();
            } else if ev == WM_CONTEXTMENU || ev == WM_RBUTTONUP {
                open_tray_menu(hwnd);
            }
            0
        }
        WM_APP_REZORDER => {
            for f in &app().fences {
                fence_ui::reorder(f.hwnd);
            }
            0
        }
        WM_APP_SHOW => {
            if app().hidden_all {
                toggle_all();
            }
            for f in &app().fences {
                fence_ui::set_visible(f.hwnd, true);
                fence_ui::reorder(f.hwnd);
            }
            SetForegroundWindow(hwnd);
            0
        }
        WM_APP_FS => {
            refresh_all();
            0
        }
        WM_TIMER => {
            if wp == TIMER_SYNC {
                tick();
            }
            0
        }
        WM_DISPLAYCHANGE => {
            for f in &app().fences {
                fence_ui::set_visible(f.hwnd, !app().hidden_all);
                fence_ui::reorder(f.hwnd);
            }
            0
        }
        WM_DESTROY => {
            PostQuitMessage(0);
            0
        }
        _ => DefWindowProcW(hwnd, msg, wp, lp),
    }
}

unsafe fn tick() {
    dlog("TICK");
    refresh_all();

    // 全屏程序：把围栏藏起来
    let fs = shell::is_fullscreen_app();
    let a = app();
    if fs && !a.fullscreen_hidden {
        for f in &a.fences {
            fence_ui::set_visible(f.hwnd, false);
        }
        a.fullscreen_hidden = true;
    } else if !fs && a.fullscreen_hidden {
        if !a.hidden_all {
            for f in &a.fences {
                fence_ui::set_visible(f.hwnd, true);
            }
        }
        a.fullscreen_hidden = false;
    }

    // 图标状态同步：用户手动开了「显示桌面图标」就再关掉
    if !fs && !app().sandbox {
        let a = app();
        if a.cfg.global.hide_icons && matches!(a.cfg.global.icon_policy, IconPolicy::Auto) {
            if shell::icons_visible() == Some(true) {
                shell::set_icons_visible(false);
                a.icons_hidden_by_us = true;
                mark_icons_hidden();
            }
        }
    }
}

unsafe extern "system" fn win_event_proc(
    _hook: HANDLE,
    _event: DWORD,
    _hwnd: HWND,
    _id_object: LONG,
    _id_child: LONG,
    _thread: DWORD,
    _time: DWORD,
) {
    if APP.is_null() {
        return;
    }
    let a = app();
    if !a.hctrl.is_null() {
        PostMessageW(a.hctrl, WM_APP_REZORDER, 0, 0);
    }
}

// ------------------------------------------------------------------ 菜单与命令

pub unsafe fn show_fence_menu(hwnd: HWND, pt: POINT) {
    let (collapsed, locked) = match app().fences.iter().find(|f| f.hwnd == hwnd) {
        Some(f) => (f.cfg.collapsed, f.cfg.locked),
        None => return,
    };
    let owner = app().hctrl;
    let menu = CreatePopupMenu();
    if menu.is_null() {
        return;
    }
    append(menu, 2001, if collapsed { i18n::t("expand") } else { i18n::t("collapse") }, false);
    append(menu, 2002, if locked { i18n::t("unlock") } else { i18n::t("lock") }, false);
    append_sep(menu);
    append(menu, 2006, i18n::t("arrange"), false);
    append(menu, 2007, i18n::t("tidy"), false);
    append(menu, 2003, i18n::t("new_fence"), false);
    append(menu, 2008, i18n::t("delete_fence"), false);
    append(menu, 2009, i18n::t("fence_props"), false);
    append(menu, 2004, i18n::t("hide_all"), false);
    append_sep(menu);
    append(menu, 2005, i18n::t("exit_fence"), false);

    SetForegroundWindow(owner);
    let cmd = TrackPopupMenu(
        menu,
        TPM_RETURNCMD | TPM_RIGHTBUTTON,
        pt.x,
        pt.y,
        0,
        owner,
        std::ptr::null(),
    ) as u32;
    PostMessageW(owner, WM_NULL, 0, 0);
    DestroyMenu(menu);

    match cmd {
        2001 => {
            if let Some(f) = fence_mut(hwnd) {
                let c = f.cfg.collapsed;
                f.set_collapsed(!c);
            }
            save_config();
        }
        2002 => {
            if let Some(f) = fence_mut(hwnd) {
                f.cfg.locked = !f.cfg.locked;
                f.redraw();
            }
            save_config();
        }
        2006 => arrange_desktop(),
        2007 => tidy_now(),
        2003 => new_fence(),
        2008 => delete_fence(hwnd),
        2009 => edit_fence_props(hwnd),
        2004 => {
            app().hidden_all = true;
            for f in &app().fences {
                fence_ui::set_visible(f.hwnd, false);
            }
        }
        2005 => quit(),
        _ => {}
    }
}

/// 右键某个文件时的菜单。
pub unsafe fn show_item_menu(hwnd: HWND, index: usize, pt: POINT) {
    let info = fence_mut(hwnd).and_then(|f| {
        f.items.get(index).map(|i| (i.path.clone(), i.is_dir))
    });
    let (path, is_dir) = match info {
        Some(v) => v,
        None => return,
    };
    let owner = app().hctrl;
    let menu = CreatePopupMenu();
    if menu.is_null() {
        return;
    }
    append(menu, 3001, if is_dir { i18n::t("open_folder") } else { i18n::t("open") }, false);
    append(menu, 3002, i18n::t("reveal"), false);
    append_sep(menu);
    append(menu, 3005, i18n::t("restore_origin"), false);
    append(menu, 3003, i18n::t("restore_desktop"), false);
    append(menu, 3004, i18n::t("delete"), false);

    SetForegroundWindow(owner);
    let cmd = TrackPopupMenu(
        menu,
        TPM_RETURNCMD | TPM_RIGHTBUTTON,
        pt.x,
        pt.y,
        0,
        owner,
        std::ptr::null(),
    ) as u32;
    PostMessageW(owner, WM_NULL, 0, 0);
    DestroyMenu(menu);

    match cmd {
        3001 => shell::open_path(&path),
        3002 => shell::select_in_explorer(&path),
        3005 => {
            let origin = journal::origin_of(&path)
                .and_then(|p| p.parent().map(|d| d.to_path_buf()))
                .unwrap_or_else(config::desktop_dir);
            if !move_to_dir(&path, &origin) {
                tray::balloon(owner, "桌面围栏", "放回原位置失败，目标目录可能不存在或文件被占用。");
            }
            refresh_all();
        }
        3003 => {
            let desk = config::desktop_dir();
            if !move_to_dir(&path, &desk) {
                tray::balloon(owner, "桌面围栏", "移回桌面失败，文件可能正被占用。");
            }
            refresh_all();
        }
        3004 => {
            if shell::delete_to_recycle(&path) {
                refresh_all();
            }
        }
        _ => {}
    }
}

/// 把文件从围栏移回桌面，重名自动加 (2)。
unsafe fn move_to_dir(path: &Path, desk: &Path) -> bool {
    let file_name = match path.file_name() {
        Some(n) => n.to_string_lossy().to_string(),
        None => return false,
    };
    let mut target = desk.join(&file_name);
    if target.exists() {
        let stem = path
            .file_stem()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_else(|| file_name.clone());
        let ext = path
            .extension()
            .map(|e| format!(".{}", e.to_string_lossy()))
            .unwrap_or_default();
        let mut n = 2;
        loop {
            target = desk.join(format!("{} ({}){}", stem, n, ext));
            if !target.exists() {
                break;
            }
            n += 1;
            if n > 999 {
                return false;
            }
        }
    }
    let (a, b) = (w(&path.to_string_lossy()), w(&target.to_string_lossy()));
    MoveFileExW(a.as_ptr(), b.as_ptr(), MOVEFILE_COPY_ALLOWED) != 0
}

/// 重新扫描所有围栏，内容变了才重绘。
unsafe fn refresh_all() {
    for f in &mut app().fences {
        if f.refresh() {
            f.redraw();
        }
    }
}

unsafe fn append(menu: HMENU, id: u32, text: &str, checked: bool) {
    let flags = MF_STRING | if checked { MF_CHECKED } else { 0 };
    let t = w(text);
    AppendMenuW(menu, flags, id as usize, t.as_ptr());
}

unsafe fn append_sep(menu: HMENU) {
    AppendMenuW(menu, MF_SEPARATOR, 0, std::ptr::null());
}

unsafe fn open_tray_menu(hwnd: HWND) {
    let icons_hidden = matches!(shell::icons_visible(), Some(false)) || app().cfg.global.hide_icons;
    let st = tray::MenuState {
        hidden_all: app().hidden_all,
        dock: app().cfg.global.dock_mode,
        all_locked: !app().fences.is_empty() && app().fences.iter().all(|f| f.cfg.locked),
        icons_hidden,
        autostart: tray::autostart_enabled(),
    };
    let cmd = tray::show_menu(hwnd, &st);
    handle_cmd(cmd);
}

unsafe fn handle_cmd(cmd: u32) {
    match cmd {
        tray::CMD_TOGGLE_ALL => toggle_all(),
        tray::CMD_DOCK => {
            let v = !app().cfg.global.dock_mode;
            app().cfg.global.dock_mode = v;
            save_config();
            tray::balloon(app().hctrl, "桌面围栏", "边缘 Dock 在 M5 交付，当前先记录开关状态。");
        }
        tray::CMD_LOCK_ALL => set_lock_all(true),
        tray::CMD_UNLOCK_ALL => set_lock_all(false),
        tray::CMD_HIDE_ICONS => toggle_icons(),
        tray::CMD_NEW_FENCE => new_fence(),
        tray::CMD_TIDY => tidy_now(),
        tray::CMD_ARRANGE => arrange_desktop(),
        tray::CMD_UNDO => {
            let (ok, skip) = journal::undo_last();
            refresh_all();
            let msg = if ok == 0 && skip == 0 {
                i18n::t("undo_none").to_string()
            } else {
                format!("{} {} / {} {}", i18n::t("undo_done"), ok, i18n::t("tidy_skip"), skip)
            };
            tray::balloon(app().hctrl, i18n::t("app"), &msg);
        }
        tray::CMD_SETTINGS => settings_ui::open(app().hinst, app().hctrl),
        tray::CMD_AUTOSTART => toggle_autostart(),
        tray::CMD_EXIT => quit(),
        _ => {}
    }
}

unsafe fn toggle_all() {
    let a = app();
    a.hidden_all = !a.hidden_all;
    let show = !a.hidden_all;
    for f in &a.fences {
        fence_ui::set_visible(f.hwnd, show);
    }
    if show {
        for f in &a.fences {
            fence_ui::reorder(f.hwnd);
        }
    }
}

unsafe fn set_lock_all(locked: bool) {
    let a = app();
    for f in &mut a.fences {
        f.cfg.locked = locked;
        f.redraw();
    }
    save_config();
}

/// 一次性把桌面上「已存在」的文件整理进围栏。
/// 先扫描出计划、弹确认，再执行；每一步都写 journal，可以撤销。
unsafe fn tidy_now() {
    let a = app();
    let rules: Vec<(PathBuf, Vec<String>)> = a
        .cfg
        .fences
        .iter()
        .map(|f| (f.dir.clone(), f.rules.clone()))
        .collect();
    let sources: Vec<PathBuf> = [config::desktop_dir(), config::public_desktop_dir()]
        .into_iter()
        .filter(|d| d.exists())
        .collect();
    let lnk = a.cfg.global.classify_lnk;
    let dirs = a.cfg.global.include_dirs;
    let default_dir = a
        .cfg
        .fences
        .first()
        .map(|f| f.dir.clone())
        .unwrap_or_else(config::fences_root);
    let plan = classify::plan_tidy(&sources, &rules, lnk, dirs, &default_dir);
    if plan.is_empty() {
        tray::balloon(a.hctrl, i18n::t("app"), i18n::t("tidy_none"));
        return;
    }
    let msg = format!(
        "{}\n[{}]",
        i18n::t("tidy_confirm"),
        plan.len()
    );
    let r = MessageBoxW(
        a.hctrl,
        w(&msg).as_ptr(),
        w(i18n::t("tidy")).as_ptr(),
        MB_YESNO | MB_ICONQUESTION | MB_SETFOREGROUND,
    );
    if r != IDYES {
        return;
    }
    let batch = journal::next_batch();
    let dry = a.cfg.global.dry_run;
    let (ok, skip) = classify::apply_plan(plan, batch, dry);
    refresh_all();
    tray::balloon(
        a.hctrl,
        i18n::t("app"),
        &format!(
            "{}{} {} / {} {}",
            if dry { "[dry-run] " } else { "" },
            i18n::t("tidy_done"),
            ok,
            i18n::t("tidy_skip"),
            skip
        ),
    );
}

/// 只排列桌面图标位置，不移动任何文件。
unsafe fn arrange_desktop() {
    let ok = shell::arrange_desktop_icons();
    tray::balloon(
        app().hctrl,
        i18n::t("app"),
        if ok {
            i18n::t("arranged")
        } else {
            i18n::t("arrange_fail")
        },
    );
}

unsafe fn toggle_icons() {
    let want = match shell::icons_visible() {
        Some(v) => !v,
        None => !app().cfg.global.hide_icons,
    };
    let ok = shell::set_icons_visible(want);
    let a = app();
    a.cfg.global.hide_icons = want;
    a.icons_hidden_by_us = want;
    if want {
        mark_icons_hidden();
    } else {
        clear_icon_marker();
    }
    if !ok {
        tray::balloon(a.hctrl, "桌面围栏", "系统不允许修改桌面图标显示状态。");
    }
    save_config();
}

unsafe fn toggle_autostart() {
    let on = !tray::autostart_enabled();
    let ok = tray::set_autostart(on);
    if ok {
        app().cfg.global.autostart = on;
        save_config();
    } else {
        tray::balloon(app().hctrl, "桌面围栏", "写入开机自启失败，可能被安全软件拦截。");
    }
}

/// 启动/重启桌面归类监听。围栏增删后要重跑，否则规则和目标目录是旧的。
unsafe fn start_watcher() {
    // Drop 旧 Watcher 会置停止事件并等待线程退出，不产生竞态。
    app().watcher = None;
    if app().fences.is_empty() {
        return;
    }
    let rules: Vec<(PathBuf, Vec<String>)> = app()
        .cfg
        .fences
        .iter()
        .map(|f| (f.dir.clone(), f.rules.clone()))
        .collect();
    let desktop = config::desktop_dir();
    let _ = std::fs::create_dir_all(&desktop);
    let lnk = app().cfg.global.classify_lnk;
    let dirs = app().cfg.global.include_dirs;
    let dry = app().cfg.global.dry_run;
    let default_dir = app()
        .cfg
        .fences
        .first()
        .map(|f| f.dir.clone())
        .unwrap_or_else(config::fences_root);
    let batch = journal::next_batch();
    let hctrl = app().hctrl;
    app().watcher = classify::start(desktop, rules, lnk, dirs, default_dir, dry, hctrl, batch);
}

/// 编辑围栏属性：名称、存放目录、写入的文件类型、列数。
unsafe fn edit_fence_props(hwnd: HWND) {
    let a = app();
    let idx = match a.fences.iter().position(|f| f.hwnd == hwnd) {
        Some(i) => i,
        None => return,
    };
    let Some(cf) = a.cfg.fences.get(idx).cloned() else {
        return;
    };
    let initial = fence_dlg::FenceProps {
        name: cf.name.clone(),
        dir: cf.dir.clone(),
        rules: cf.rules.clone(),
        cols: cf.cols,
    };
    let (hinst, owner) = (a.hinst, a.hctrl);
    let Some(p) = fence_dlg::show(hinst, owner, initial) else {
        return;
    };
    apply_fence_props(idx, p);
}

/// 把对话框结果落到围栏与配置上，并让监听用上新规则。
unsafe fn apply_fence_props(idx: usize, p: fence_dlg::FenceProps) {
    let a = app();
    // 目录不存在就建出来，避免扫不到文件
    let _ = std::fs::create_dir_all(&p.dir);
    if let Some(f) = a.fences.get_mut(idx) {
        f.cfg.name = p.name.clone();
        f.cfg.dir = p.dir.clone();
        f.cfg.rules = p.rules.clone();
        f.cfg.cols = p.cols;
        f.refresh();
        f.redraw();
    }
    if let Some(c) = a.cfg.fences.get_mut(idx) {
        c.name = p.name;
        c.dir = p.dir;
        c.rules = p.rules;
        c.cols = p.cols;
    }
    let saved = a.cfg.save();
    // 规则和目标目录都变了，必须重启监听
    start_watcher();
}

unsafe fn new_fence() {
    let a = app();
    let mut n = 1;
    let name = loop {
        let cand = format!("新围栏 {}", n);
        if !a.cfg.fences.iter().any(|f| f.name == cand) {
            break cand;
        }
        n += 1;
    };
    let initial = fence_dlg::FenceProps {
        dir: config::fences_root().join(&name),
        rules: config::default_rules(),
        cols: 4,
        name,
    };
    let (hinst, owner) = (a.hinst, a.hctrl);
    let Some(p) = fence_dlg::show(hinst, owner, initial) else {
        return;
    };

    let work = primary_work();
    let id = a.cfg.fences.iter().map(|f| f.id).max().unwrap_or(0) + 1;
    let dpi_now = fence_ui::default_dpi();
    let w_ = fence_ui::dip_pub(400, dpi_now);
    let h_ = fence_ui::dip_pub(252, dpi_now);
    let cf = FenceCfg {
        id,
        name: p.name.clone(),
        dir: p.dir.clone(),
        x: work.left + (work.w() - w_) / 2,
        y: work.top + (work.h() - h_) / 2,
        w: w_,
        h: h_,
        h_expanded: h_,
        cols: p.cols,
        opacity: a.cfg.global.opacity,
        collapsed: false,
        sort: SortMode::Name,
        locked: false,
        snap: true,
        color: 0x2F3442,
        rules: p.rules,
    };
    let _ = std::fs::create_dir_all(&cf.dir);
    let hinst = a.hinst;
    let dpi = fence_ui::default_dpi();
    if let Some(f) = fence_ui::create(hinst, cf.clone(), dpi) {
        a.cfg.fences.push(cf);
        a.fences.push(f);
        if let Some(f) = a.fences.last() {
            fence_ui::reorder(f.hwnd);
            fence_ui::set_visible(f.hwnd, !a.hidden_all);
        }
        save_config();
        start_watcher();
    }
}

unsafe fn delete_fence(hwnd: HWND) {
    let a = app();
    let idx = match a.fences.iter().position(|f| f.hwnd == hwnd) {
        Some(i) => i,
        None => return,
    };
    let Some(cf) = a.cfg.fences.get(idx).cloned() else {
        return;
    };

    // 只统计个数，一次性读取，不进热路径
    let n = std::fs::read_dir(&cf.dir)
        .map(|rd| rd.flatten().count())
        .unwrap_or(0);

    let files_line = if n == 0 {
        i18n::t("delete_fence_empty").to_string()
    } else {
        format!(
            "{} {} {}：{}",
            i18n::t("delete_fence_files"),
            n,
            i18n::t("delete_fence_unit"),
            cf.dir.display()
        )
    };

    let body = format!(
        "{}「{}」？\n\n{}\n\n{}\n{}\n{}",
        i18n::t("delete_fence_title"),
        cf.name,
        files_line,
        i18n::t("delete_fence_yes"),
        i18n::t("delete_fence_no"),
        i18n::t("delete_fence_cancel"),
    );

    let r = MessageBoxW(
        a.hctrl,
        w(&body).as_ptr(),
        w(i18n::t("delete_fence_title")).as_ptr(),
        MB_YESNOCANCEL | MB_ICONQUESTION | MB_SETFOREGROUND,
    );
    if r == IDCANCEL {
        return;
    }
    delete_fence_do(idx, r == IDYES);
}

/// 真正执行删除。`to_desktop` 决定文件去向：true＝放回桌面，false＝原地保留。
/// 与对话框分离，便于自动化测试直接驱动。
unsafe fn delete_fence_do(idx: usize, to_desktop: bool) {
    let a = app();
    let Some(cf) = a.cfg.fences.get(idx).cloned() else {
        return;
    };

    if to_desktop {
        let desktop = config::desktop_dir();
        let _ = std::fs::create_dir_all(&desktop);
        let plan: Vec<(PathBuf, PathBuf)> = std::fs::read_dir(&cf.dir)
            .map(|rd| {
                rd.flatten()
                    .map(|e| (e.path(), desktop.clone()))
                    .collect()
            })
            .unwrap_or_default();
        let batch = journal::next_batch();
        let (ok, skip) = classify::apply_plan(plan, batch, a.cfg.global.dry_run);
        dlog(&format!(
            "DELETE 围栏「{}」文件回桌面 ok={} skip={}",
            cf.name, ok, skip
        ));
    }

    // 拆窗口 + 配置条目（cfg.fences 与 fences 的索引是同步的）
    if idx < a.fences.len() {
        let f = a.fences.remove(idx);
        DestroyWindow(f.hwnd);
    }
    if idx < a.cfg.fences.len() {
        a.cfg.fences.remove(idx);
    }
    let _ = a.cfg.save();

    // 规则与默认目标目录都可能变了，重启监听
    start_watcher();

    let msg = if to_desktop {
        i18n::t("delete_fence_done_desktop").to_string()
    } else {
        format!("{}{}", i18n::t("delete_fence_done_keep"), cf.dir.display())
    };
    tray::balloon(a.hctrl, i18n::t("app"), &msg);
}

unsafe fn quit() {
    let a = app();
    if a.quitting {
        return;
    }
    a.quitting = true;
    save_config();
    if a.icons_hidden_by_us {
        shell::set_icons_visible(true);
        clear_icon_marker();
    }
    if a.tray_ok {
        tray::remove(a.hctrl);
        a.tray_ok = false;
    }
    if !a.hctrl.is_null() {
        DestroyWindow(a.hctrl);
    }
}

// ------------------------------------------------------------------ 杂项

unsafe fn set_dpi_awareness() {
    type Fn = unsafe extern "system" fn(isize) -> BOOL;
    if let Some(p) = proc("user32.dll", "SetProcessDpiAwarenessContext") {
        let f: Fn = std::mem::transmute(p);
        // DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2 == -4
        if f(-4) != 0 {
            return;
        }
    }
    SetProcessDPIAware();
}

unsafe fn primary_work() -> RECT {
    let mon = MonitorFromWindow(null_mut(), MONITOR_DEFAULTTONEAREST);
    let mut mi: MONITORINFO = std::mem::zeroed();
    mi.cbSize = size_of::<MONITORINFO>() as u32;
    if !mon.is_null() && GetMonitorInfoW(mon, &mut mi) != 0 {
        mi.rcWork
    } else {
        RECT::new(0, 0, 1280, 720)
    }
}

#[allow(dead_code)]
fn _unused(p: PathBuf) -> PathBuf {
    p
}
