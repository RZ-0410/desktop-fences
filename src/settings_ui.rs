//! 设置窗口：语言切换 + 几个常用开关。
//!
//! 用系统原生控件（Button/Static），不自绘。理由：设置窗口只此一处，
//! 自绘成本高且不值得；围栏本体已经是自绘的，界面质感够用了。
//!
//! 关键要求是「切换语言后立刻生效」：确定按钮里先 `set_lang`，再重绘所有围栏、
//! 重建托盘菜单文案，最后落盘。

use std::ptr::null_mut;

use crate::i18n::{self, Lang};
use crate::winapi::*;

pub const CLASS_NAME: &str = "DFSettings";

const ID_LANG_ZH: usize = 2001;
const ID_LANG_EN: usize = 2002;
const ID_AUTOSTART: usize = 2003;
const ID_HIDE_ICONS: usize = 2004;
const ID_INCLUDE_DIRS: usize = 2005;
const ID_OK: usize = 2006;
const ID_CANCEL: usize = 2007;

struct Settings {
    hwnd: HWND,
    zh: HWND,
    en: HWND,
    autostart: HWND,
    hide_icons: HWND,
    include_dirs: HWND,
}

static mut SETTINGS: *mut Settings = std::ptr::null_mut();

fn settings_mut() -> Option<&'static mut Settings> {
    unsafe {
        if SETTINGS.is_null() {
            None
        } else {
            Some(&mut *SETTINGS)
        }
    }
}

/// 打开设置窗口（每次只开一个）。
pub unsafe fn open(hinst: HINSTANCE, owner: HWND) {
    if !SETTINGS.is_null() {
        let h = (*SETTINGS).hwnd;
        ShowWindow(h, SW_SHOW);
        SetForegroundWindow(h);
        return;
    }

    let cls = w(CLASS_NAME);
    let wc = WNDCLASSEXW {
        cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
        style: 0,
        lpfnWndProc: Some(proc),
        cbClsExtra: 0,
        cbWndExtra: 0,
        hInstance: hinst,
        hIcon: LoadIconW(hinst, 2 as LPCWSTR),
        hCursor: LoadCursorW(null_mut(), IDC_ARROW),
        hbrBackground: (COLOR_BTNFACE + 1) as HBRUSH,
        lpszMenuName: null_mut(),
        lpszClassName: cls.as_ptr(),
        hIconSm: LoadIconW(hinst, 2 as LPCWSTR),
    };
    RegisterClassExW(&wc);

    let (w_, h_) = (380, 300);
    let hwnd = CreateWindowExW(
        0,
        cls.as_ptr(),
        w("设置").as_ptr(),
        WS_POPUP | WS_CAPTION | WS_SYSMENU,
        200,
        200,
        w_,
        h_,
        owner,
        null_mut(),
        hinst,
        null_mut(),
    );
    if hwnd.is_null() {
        return;
    }

    let font = GetStockObject(DEFAULT_GUI_FONT);

    let mut mk = |cls: &str, text: &str, id: usize, x: i32, y: i32, ww: i32, hh: i32, style: DWORD| {
        let h = CreateWindowExW(
            0,
            w(cls).as_ptr(),
            w(text).as_ptr(),
            WS_CHILD | WS_VISIBLE | style,
            x,
            y,
            ww,
            hh,
            hwnd,
            id as HMENU,
            hinst,
            null_mut(),
        );
        if !h.is_null() {
            SendMessageW(h, 0x0030, font as WPARAM, 1); // WM_SETFONT
        }
        h
    };

    let st = Box::new(Settings {
        hwnd,
        zh: mk("Button", "中文", ID_LANG_ZH, 28, 46, 140, 26, BS_AUTORADIOBUTTON | WS_TABSTOP | WS_GROUP),
        en: mk("Button", "English", ID_LANG_EN, 28, 78, 140, 26, BS_AUTORADIOBUTTON | WS_TABSTOP),
        autostart: mk("Button", "", ID_AUTOSTART, 28, 132, 300, 26, BS_AUTOCHECKBOX | WS_TABSTOP),
        hide_icons: mk("Button", "", ID_HIDE_ICONS, 28, 164, 300, 26, BS_AUTOCHECKBOX | WS_TABSTOP),
        include_dirs: mk("Button", "", ID_INCLUDE_DIRS, 28, 196, 300, 26, BS_AUTOCHECKBOX | WS_TABSTOP),
    });
    SETTINGS = Box::into_raw(st);

    mk("Static", "", 0, 20, 18, 330, 20, 0); // 占位，标题靠窗口标题
    mk("Button", "", ID_OK, 180, 240, 84, 30, WS_TABSTOP);
    mk("Button", "", ID_CANCEL, 272, 240, 84, 30, WS_TABSTOP);

    apply_texts();
    sync_from_config();
    ShowWindow(hwnd, SW_SHOW);
}

/// 语言切换后刷新窗口上的文案。
pub fn apply_texts() {
    let Some(s) = settings_mut() else { return };
    unsafe {
        SetWindowTextW(s.hwnd, w(i18n::t("settings_title")).as_ptr());
        let set = |h: HWND, key: &'static str| SetWindowTextW(h, w(i18n::t(key)).as_ptr());
        set(s.autostart, "autostart");
        set(s.hide_icons, "hide_icons");
        set(s.include_dirs, "include_dirs");
    }
}

unsafe fn sync_from_config() {
    let Some(s) = settings_mut() else { return };
    let a = crate::app();
    let check = |h: HWND, on: bool| SendMessageW(h, BM_SETCHECK, if on { BST_CHECKED } else { BST_UNCHECKED }, 0);
    check(s.zh, a.cfg.global.lang == Lang::Zh);
    check(s.en, a.cfg.global.lang == Lang::En);
    check(s.autostart, a.cfg.global.autostart);
    check(s.hide_icons, a.cfg.global.hide_icons);
    check(s.include_dirs, a.cfg.global.include_dirs);
}

unsafe fn checked(h: HWND) -> bool {
    SendMessageW(h, BM_GETCHECK, 0, 0) == BST_CHECKED as LRESULT
}

unsafe fn on_ok() {
    let Some(s) = settings_mut() else { return };
    let lang = if checked(s.en) { Lang::En } else { Lang::Zh };

    let a = crate::app();
    let autostart = checked(s.autostart);
    let hide_icons = checked(s.hide_icons);
    let include_dirs = checked(s.include_dirs);

    // 语言：立刻生效
    i18n::set_lang(lang);
    a.cfg.global.lang = lang;

    if autostart != a.cfg.global.autostart {
        crate::tray::set_autostart(autostart);
        a.cfg.global.autostart = autostart;
    }
    if hide_icons != a.cfg.global.hide_icons {
        a.cfg.global.hide_icons = hide_icons;
        if hide_icons {
            shell_show(false);
        } else {
            shell_show(true);
        }
    }
    a.cfg.global.include_dirs = include_dirs;

    let _ = a.cfg.save();

    // 让所有界面重绘/重译
    for f in &mut a.fences {
        f.redraw();
    }
    apply_texts();
    crate::tray::balloon(a.hctrl, i18n::t("app"), i18n::t("settings_note"));
}

unsafe fn shell_show(want: bool) {
    let ok = crate::shell::set_icons_visible(want);
    let a = crate::app();
    a.icons_hidden_by_us = !want && ok;
    if !want && ok {
        crate::mark_icons_hidden();
    } else {
        crate::clear_icon_marker();
    }
}

unsafe extern "system" fn proc(hwnd: HWND, msg: UINT, wp: WPARAM, lp: LPARAM) -> LRESULT {
    match msg {
        WM_COMMAND => {
            let id = wp & 0xffff;
            match id {
                x if x == ID_OK as usize => {
                    on_ok();
                    DestroyWindow(hwnd);
                }
                x if x == ID_CANCEL as usize => {
                    DestroyWindow(hwnd);
                }
                x if x == ID_LANG_ZH as usize || x == ID_LANG_EN as usize => {
                    // 点选语言即时预览窗口文案
                    let Some(s) = settings_mut() else { return 0 };
                    let lang = if x == ID_LANG_EN as usize { Lang::En } else { Lang::Zh };
                    i18n::set_lang(lang);
                    apply_texts();
                    let _ = s;
                }
                _ => {}
            }
            return 0;
        }
        WM_CLOSE => {
            DestroyWindow(hwnd);
            return 0;
        }
        WM_NCDESTROY => {
            unsafe {
                if !SETTINGS.is_null() {
                    drop(Box::from_raw(SETTINGS));
                    SETTINGS = std::ptr::null_mut();
                }
            }
            return 0;
        }
        _ => {}
    }
    DefWindowProcW(hwnd, msg, wp, lp)
}
