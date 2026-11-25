//! 托盘图标、托盘菜单、开机自启。

use std::ffi::c_void;
use std::mem::size_of;
use std::ptr::null_mut;

use crate::winapi::*;

pub const CMD_TOGGLE_ALL: u32 = 1001;
pub const CMD_DOCK: u32 = 1002;
pub const CMD_LOCK_ALL: u32 = 1003;
pub const CMD_UNLOCK_ALL: u32 = 1004;
pub const CMD_HIDE_ICONS: u32 = 1005;
pub const CMD_NEW_FENCE: u32 = 1006;
pub const CMD_UNDO: u32 = 1007;
pub const CMD_SETTINGS: u32 = 1008;
pub const CMD_AUTOSTART: u32 = 1009;
pub const CMD_TIDY: u32 = 1011;
pub const CMD_ARRANGE: u32 = 1012;
pub const CMD_EXIT: u32 = 1010;

const RUN_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";
const RUN_VALUE: &str = "DesktopFences";

pub struct MenuState {
    pub hidden_all: bool,
    pub dock: bool,
    pub all_locked: bool,
    pub icons_hidden: bool,
    pub autostart: bool,
}

fn base(hwnd: HWND) -> NOTIFYICONDATAW {
    let mut n: NOTIFYICONDATAW = unsafe { std::mem::zeroed() };
    n.cbSize = size_of::<NOTIFYICONDATAW>() as u32;
    n.hWnd = hwnd;
    n.uID = 1;
    n
}

pub unsafe fn add(hwnd: HWND, tip: &str) -> bool {
    let mut n = base(hwnd);
    n.uFlags = NIF_MESSAGE | NIF_ICON | NIF_TIP | NIF_SHOWTIP;
    n.uCallbackMessage = crate::WM_APP_TRAY;
    // 用程序自己的图标（资源 ID 2），别用系统通用图标，否则托盘里认不出来
    let hinst = GetModuleHandleW(null_mut());
    let mut icon = LoadIconW(hinst, 2 as LPCWSTR);
    if icon.is_null() {
        icon = LoadIconW(null_mut(), IDI_APPLICATION);
    }
    n.hIcon = icon;
    fill_wide(&mut n.szTip, tip);
    if Shell_NotifyIconW(NIM_ADD, &n) == 0 {
        return false;
    }
    n.uVersion = NOTIFYICON_VERSION_4;
    Shell_NotifyIconW(NIM_SETVERSION, &n);
    true
}

pub unsafe fn remove(hwnd: HWND) {
    let n = base(hwnd);
    Shell_NotifyIconW(NIM_DELETE, &n);
}

pub unsafe fn set_tip(hwnd: HWND, tip: &str) {
    let mut n = base(hwnd);
    n.uFlags = NIF_TIP | NIF_SHOWTIP;
    fill_wide(&mut n.szTip, tip);
    Shell_NotifyIconW(NIM_MODIFY, &n);
}

/// 气泡提示。失败不重试，不弹对话框。
pub unsafe fn balloon(hwnd: HWND, title: &str, text: &str) {
    let mut n = base(hwnd);
    n.uFlags = NIF_INFO;
    n.dwInfoFlags = NIIF_INFO;
    fill_wide(&mut n.szInfoTitle, title);
    fill_wide(&mut n.szInfo, text);
    Shell_NotifyIconW(NIM_MODIFY, &n);
}

fn item(menu: HMENU, id: u32, text: &str, checked: bool, enabled: bool) {
    let mut flags = MF_STRING;
    if checked {
        flags |= MF_CHECKED;
    }
    if !enabled {
        flags |= MF_GRAYED;
    }
    let t = w(text);
    unsafe {
        AppendMenuW(menu, flags, id as usize, t.as_ptr());
    }
}

fn sep(menu: HMENU) {
    unsafe {
        AppendMenuW(menu, MF_SEPARATOR, 0, std::ptr::null());
    }
}

/// 弹出托盘菜单，返回被点中的命令（`TPM_RETURNCMD`），0 表示取消。
pub unsafe fn show_menu(hwnd: HWND, st: &MenuState) -> u32 {
    let menu = CreatePopupMenu();
    if menu.is_null() {
        return 0;
    }
    item(menu, CMD_TOGGLE_ALL, if st.hidden_all { crate::i18n::t("show_all") } else { crate::i18n::t("hide_all") }, false, true);
    item(menu, CMD_DOCK, crate::i18n::t("dock"), st.dock, true);
    sep(menu);
    item(
        menu,
        if st.all_locked { CMD_UNLOCK_ALL } else { CMD_LOCK_ALL },
        if st.all_locked { crate::i18n::t("unlock_all") } else { crate::i18n::t("lock_all") },
        false,
        true,
    );
    item(menu, CMD_HIDE_ICONS, crate::i18n::t("hide_icons"), st.icons_hidden, true);
    item(menu, CMD_NEW_FENCE, crate::i18n::t("new_fence"), false, true);
    item(menu, CMD_TIDY, crate::i18n::t("tidy"), false, true);
    item(menu, CMD_ARRANGE, crate::i18n::t("arrange"), false, true);
    item(menu, CMD_UNDO, crate::i18n::t("undo"), false, st.last_batch_ok());
    sep(menu);
    item(menu, CMD_SETTINGS, crate::i18n::t("settings"), false, true);
    item(menu, CMD_AUTOSTART, crate::i18n::t("autostart"), st.autostart, true);
    sep(menu);
    item(menu, CMD_EXIT, crate::i18n::t("exit"), false, true);

    let mut pt = POINT::default();
    GetCursorPos(&mut pt);
    SetForegroundWindow(hwnd);
    let cmd = TrackPopupMenu(
        menu,
        TPM_RETURNCMD | TPM_RIGHTBUTTON | TPM_BOTTOMALIGN,
        pt.x,
        pt.y,
        0,
        hwnd,
        std::ptr::null(),
    ) as u32;
    PostMessageW(hwnd, WM_NULL, 0, 0);
    DestroyMenu(menu);
    cmd
}

impl MenuState {
    fn last_batch_ok(&self) -> bool {
        true
    }
}

// ------------------------------------------------------------------ 开机自启

unsafe fn run_key(open_write: bool) -> HKEY {
    let sub = w(RUN_KEY);
    let mut hk: HKEY = null_mut();
    let mut disp: DWORD = 0;
    let rc = RegCreateKeyExW(
        HKEY_CURRENT_USER,
        sub.as_ptr(),
        0,
        null_mut(),
        REG_OPTION_NON_VOLATILE,
        if open_write { KEY_SET_VALUE } else { KEY_QUERY_VALUE },
        null_mut(),
        &mut hk,
        &mut disp,
    );
    if rc != ERROR_SUCCESS {
        return null_mut();
    }
    hk
}

pub unsafe fn autostart_enabled() -> bool {
    let hk = run_key(false);
    if hk.is_null() {
        return false;
    }
    let name = w(RUN_VALUE);
    let mut buf = [0u8; 1024];
    let mut cb = buf.len() as DWORD;
    let rc = RegQueryValueExW(hk, name.as_ptr(), null_mut(), null_mut(), buf.as_mut_ptr(), &mut cb);
    RegCloseKey(hk);
    rc == ERROR_SUCCESS && cb > 4
}

pub unsafe fn set_autostart(on: bool) -> bool {
    let hk = run_key(true);
    if hk.is_null() {
        return false;
    }
    let name = w(RUN_VALUE);
    let ok = if on {
        let mut exe = vec![0u16; 1024];
        let n = GetModuleFileNameW(null_mut(), exe.as_mut_ptr(), exe.len() as u32);
        exe.truncate(n as usize);
        let path = String::from_utf16_lossy(&exe);
        let val = w(&format!("\"{}\" --autostart", path));
        let bytes = std::slice::from_raw_parts(val.as_ptr() as *const u8, val.len() * 2);
        RegSetValueExW(hk, name.as_ptr(), 0, REG_SZ, bytes.as_ptr(), bytes.len() as u32)
            == ERROR_SUCCESS
    } else {
        RegDeleteValueW(hk, name.as_ptr()) == ERROR_SUCCESS
    };
    RegCloseKey(hk);
    ok
}

/// 给 Vtbl 指针腾一个明确的类型，避免到处 `*mut c_void` 互相转换时读错。
pub type Vtbl = *const *const c_void;
