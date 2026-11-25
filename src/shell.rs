//! Shell 相关：打开文件、定位、回收站、隐藏桌面图标。

use std::path::Path;
use std::ptr::null_mut;

use crate::winapi::*;

pub fn open_path(p: &Path) {
    let f = w(&p.to_string_lossy());
    let op = w("open");
    unsafe {
        ShellExecuteW(null_mut(), op.as_ptr(), f.as_ptr(), std::ptr::null(), std::ptr::null(), SW_SHOWNORMAL);
    }
}

pub fn select_in_explorer(p: &Path) {
    let exe = w("explorer.exe");
    let params = w(&format!("/select,\"{}\"", p.display()));
    let op = w("open");
    unsafe {
        ShellExecuteW(null_mut(), op.as_ptr(), exe.as_ptr(), params.as_ptr(), std::ptr::null(), SW_SHOWNORMAL);
    }
}

/// 送进回收站（`FOF_ALLOWUNDO`），单文件，不递归。
pub fn delete_to_recycle(p: &Path) -> bool {
    let mut from: Vec<u16> = p.to_string_lossy().encode_utf16().collect();
    from.push(0);
    from.push(0); // pFrom 必须双 NUL 结尾
    let op = SHFILEOPSTRUCTW {
        hwnd: null_mut(),
        wFunc: FO_DELETE,
        pFrom: from.as_ptr(),
        pTo: std::ptr::null(),
        fFlags: FOF_ALLOWUNDO | FOF_NOCONFIRMATION | FOF_SILENT | FOF_NOERRORUI,
        fAnyOperationsAborted: 0,
        hNameMappings: null_mut(),
        lpszProgressTitle: std::ptr::null(),
    };
    unsafe { SHFileOperationW(&op) == 0 }
}

// ------------------------------------------------------------------ 桌面图标

unsafe fn progman() -> HWND {
    FindWindowW(w("Progman").as_ptr(), null_mut())
}

unsafe fn def_view() -> HWND {
    let pm = progman();
    if pm.is_null() {
        return null_mut();
    }
    FindWindowExW(pm, null_mut(), w("SHELLDLL_DefView").as_ptr(), null_mut())
}

unsafe fn icon_listview() -> HWND {
    let dv = def_view();
    if dv.is_null() {
        return null_mut();
    }
    FindWindowExW(dv, null_mut(), w("SysListView32").as_ptr(), null_mut())
}

/// 当前桌面图标是否可见。`None` 表示找不到桌面窗口（Explorer 没起）。
pub unsafe fn icons_visible() -> Option<bool> {
    let lv = icon_listview();
    if lv.is_null() {
        return None;
    }
    Some(IsWindowVisible(lv) != 0)
}

/// `WM_COMMAND 0x7402` 是**切换**语义，所以必须先读状态再发。
/// 返回操作后实际状态。
pub unsafe fn set_icons_visible(want: bool) -> bool {
    if let Some(cur) = icons_visible() {
        if cur == want {
            return true;
        }
    }
    const CMD_TOGGLE_DESKTOP: usize = 0x7402;
    let pm = progman();
    if !pm.is_null() {
        SendMessageW(pm, WM_COMMAND, CMD_TOGGLE_DESKTOP, 0);
    }
    let after = icons_visible().unwrap_or(want);
    if after != want {
        // 某些版本要发给 DefView 才生效
        let dv = def_view();
        if !dv.is_null() {
            SendMessageW(dv, WM_COMMAND, CMD_TOGGLE_DESKTOP, 0);
        }
    }
    icons_visible().unwrap_or(want) == want
}

/// 整理桌面图标：开启「自动排列」并按网格重排。
/// 只动图标位置，不移动、不改名任何文件。
pub unsafe fn arrange_desktop_icons() -> bool {
    let lv = icon_listview();
    if lv.is_null() {
        return false;
    }
    // 自动排列样式：之后新图标也会自动补位
    let style = GetWindowLongW(lv, GWL_STYLE);
    SetWindowLongW(lv, GWL_STYLE, style | LVS_AUTOARRANGE);
    // 立刻重排一次，让已有图标也回到网格
    SendMessageW(lv, LVM_ARRANGE, LVA_DEFAULT as WPARAM, 0) != 0
}

/// 全屏独占程序在跑的时候把围栏藏起来，别盖在游戏上。
pub unsafe fn is_fullscreen_app() -> bool {
    let mut state: i32 = 0;
    if SHQueryUserNotificationState(&mut state) != S_OK {
        return false;
    }
    state == QUNS_RUNNING_D3D_FULL_SCREEN || state == QUNS_PRESENTATION_MODE
}
