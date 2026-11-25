//! 原生 Win32 FFI（零第三方依赖）。
//!
//! 只声明本项目真正用到的部分。所有字符串走 W 版 API，不做 ANSI 转换。
//! 结构体一律 `#[repr(C)]`，布局必须与 windows.h 完全一致 —— 错一个字段，
//! 会在运行时以极难排查的方式炸掉。

#![allow(non_snake_case, non_camel_case_types, dead_code)]

use std::ffi::c_void;

// ------------------------------------------------------------------ 基础类型

pub type BOOL = i32;
pub type UINT = u32;
pub type DWORD = u32;
pub type WORD = u16;
pub type LONG = i32;
pub type ULONG = u32;
pub type LRESULT = isize;
pub type WPARAM = usize;
pub type LPARAM = isize;
pub type ATOM = u16;
pub type HRESULT = i32;
pub type UINT_PTR = usize;

pub type HANDLE = *mut c_void;
pub type HWND = *mut c_void;
pub type HINSTANCE = *mut c_void;
pub type HMODULE = *mut c_void;
pub type HMENU = *mut c_void;
pub type HICON = *mut c_void;
pub type HCURSOR = *mut c_void;
pub type HBRUSH = *mut c_void;
pub type HFONT = *mut c_void;
pub type HDC = *mut c_void;
pub type HBITMAP = *mut c_void;
pub type HGDIOBJ = *mut c_void;
pub type HKEY = *mut c_void;
pub type HMONITOR = *mut c_void;
pub type HIMAGELIST = *mut c_void;

pub type LPCWSTR = *const u16;
pub type LPWSTR = *mut u16;
pub type LPCSTR = *const u8;
pub type LPVOID = *mut c_void;
pub type WNDPROC = Option<unsafe extern "system" fn(HWND, UINT, WPARAM, LPARAM) -> LRESULT>;
pub type WINEVENTPROC =
    Option<unsafe extern "system" fn(HANDLE, DWORD, HWND, LONG, LONG, DWORD, DWORD)>;

pub const S_OK: HRESULT = 0;
pub const E_NOINTERFACE: HRESULT = 0x8000_4002u32 as HRESULT;
pub const E_POINTER: HRESULT = 0x8000_4003u32 as HRESULT;
pub const E_FAIL: HRESULT = 0x8000_4005u32 as HRESULT;
pub const MK_CONTROL: u32 = 0x0008;

pub const INVALID_HANDLE_VALUE: HANDLE = -1isize as HANDLE;
pub const HKEY_CURRENT_USER: HKEY = 0x8000_0001usize as HKEY;

// ------------------------------------------------------------------ 结构体

#[repr(C)]
#[derive(Clone, Copy, Default, PartialEq, Eq)]
pub struct POINT {
    pub x: LONG,
    pub y: LONG,
}

#[repr(C)]
#[derive(Clone, Copy, Default, PartialEq, Eq)]
pub struct POINTL {
    pub x: LONG,
    pub y: LONG,
}

#[repr(C)]
#[derive(Clone, Copy, Default, PartialEq, Eq)]
pub struct SIZE {
    pub cx: LONG,
    pub cy: LONG,
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct RECT {
    pub left: LONG,
    pub top: LONG,
    pub right: LONG,
    pub bottom: LONG,
}

impl RECT {
    pub fn new(l: i32, t: i32, r: i32, b: i32) -> Self {
        Self { left: l, top: t, right: r, bottom: b }
    }
    pub fn w(&self) -> i32 {
        self.right - self.left
    }
    pub fn h(&self) -> i32 {
        self.bottom - self.top
    }
    pub fn contains(&self, x: i32, y: i32) -> bool {
        x >= self.left && x < self.right && y >= self.top && y < self.bottom
    }
}

#[repr(C)]
pub struct WNDCLASSEXW {
    pub cbSize: UINT,
    pub style: UINT,
    pub lpfnWndProc: WNDPROC,
    pub cbClsExtra: i32,
    pub cbWndExtra: i32,
    pub hInstance: HINSTANCE,
    pub hIcon: HICON,
    pub hCursor: HCURSOR,
    pub hbrBackground: HBRUSH,
    pub lpszMenuName: LPCWSTR,
    pub lpszClassName: LPCWSTR,
    pub hIconSm: HICON,
}

#[repr(C)]
pub struct MSG {
    pub hwnd: HWND,
    pub message: UINT,
    pub wParam: WPARAM,
    pub lParam: LPARAM,
    pub time: DWORD,
    pub pt: POINT,
    pub lPrivate: DWORD,
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct BITMAPINFOHEADER {
    pub biSize: DWORD,
    pub biWidth: LONG,
    pub biHeight: LONG,
    pub biPlanes: WORD,
    pub biBitCount: WORD,
    pub biCompression: DWORD,
    pub biSizeImage: DWORD,
    pub biXPelsPerMeter: LONG,
    pub biYPelsPerMeter: LONG,
    pub biClrUsed: DWORD,
    pub biClrImportant: DWORD,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct BITMAPINFO {
    pub bmiHeader: BITMAPINFOHEADER,
    pub bmiColors: [DWORD; 1],
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct BLENDFUNCTION {
    pub BlendOp: u8,
    pub BlendFlags: u8,
    pub SourceConstantAlpha: u8,
    pub AlphaFormat: u8,
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct GUID {
    pub data1: u32,
    pub data2: u16,
    pub data3: u16,
    pub data4: [u8; 8],
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct FILETIME {
    pub dwLowDateTime: u32,
    pub dwHighDateTime: u32,
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct SYSTEMTIME {
    pub wYear: WORD,
    pub wMonth: WORD,
    pub wDayOfWeek: WORD,
    pub wDay: WORD,
    pub wHour: WORD,
    pub wMinute: WORD,
    pub wSecond: WORD,
    pub wMilliseconds: WORD,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct WIN32_FIND_DATAW {
    pub dwFileAttributes: DWORD,
    pub ftCreationTime: FILETIME,
    pub ftLastAccessTime: FILETIME,
    pub ftLastWriteTime: FILETIME,
    pub nFileSizeHigh: DWORD,
    pub nFileSizeLow: DWORD,
    pub dwReserved0: DWORD,
    pub dwReserved1: DWORD,
    pub cFileName: [u16; 260],
    pub cAlternateFileName: [u16; 14],
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct BITMAP {
    pub bmType: LONG,
    pub bmWidth: LONG,
    pub bmHeight: LONG,
    pub bmWidthBytes: LONG,
    pub bmPlanes: WORD,
    pub bmBitsPixel: WORD,
    pub bmBits: *mut c_void,
}

#[repr(C)]
pub struct SHFILEINFOW {
    pub hIcon: HICON,
    pub iIcon: i32,
    pub dwAttributes: DWORD,
    pub szDisplayName: [u16; 260],
    pub szTypeName: [u16; 80],
}

#[repr(C)]
pub struct SHFILEOPSTRUCTW {
    pub hwnd: HWND,
    pub wFunc: UINT,
    pub pFrom: LPCWSTR,
    pub pTo: LPCWSTR,
    pub fFlags: WORD,
    pub fAnyOperationsAborted: BOOL,
    pub hNameMappings: *mut c_void,
    pub lpszProgressTitle: LPCWSTR,
}

#[repr(C)]
pub struct FILE_NOTIFY_INFORMATION {
    pub NextEntryOffset: DWORD,
    pub Action: DWORD,
    pub FileNameLength: DWORD,
    pub FileName: [u16; 1],
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct OVERLAPPED {
    pub Internal: usize,
    pub InternalHigh: usize,
    pub Offset: u32,
    pub OffsetHigh: u32,
    pub hEvent: HANDLE,
}

#[repr(C)]
pub struct MONITORINFO {
    pub cbSize: DWORD,
    pub rcMonitor: RECT,
    pub rcWork: RECT,
    pub dwFlags: DWORD,
}

#[repr(C)]
pub struct TRACKMOUSEEVENT {
    pub cbSize: DWORD,
    pub dwFlags: DWORD,
    pub hwndTrack: HWND,
    pub dwHoverTime: DWORD,
}

#[repr(C)]
pub struct NOTIFYICONDATAW {
    pub cbSize: DWORD,
    pub hWnd: HWND,
    pub uID: UINT,
    pub uFlags: UINT,
    pub uCallbackMessage: UINT,
    pub hIcon: HICON,
    pub szTip: [u16; 128],
    pub dwState: DWORD,
    pub dwStateMask: DWORD,
    pub szInfo: [u16; 256],
    pub uVersion: UINT,
    pub szInfoTitle: [u16; 64],
    pub dwInfoFlags: DWORD,
    pub guidItem: GUID,
    pub hBalloonIcon: HICON,
}

#[repr(C)]
pub struct PROCESS_MEMORY_COUNTERS {
    pub cb: DWORD,
    pub PageFaultCount: DWORD,
    pub PeakWorkingSetSize: usize,
    pub WorkingSetSize: usize,
    pub QuotaPeakPagedPoolUsage: usize,
    pub QuotaPagedPoolUsage: usize,
    pub QuotaPeakNonPagedPoolUsage: usize,
    pub QuotaNonPagedPoolUsage: usize,
    pub PagefileUsage: usize,
    pub PeakPagefileUsage: usize,
}

#[repr(C)]
pub struct FORMATETC {
    pub cfFormat: u16,
    pub ptd: *mut c_void,
    pub dwAspect: DWORD,
    pub lindex: LONG,
    pub tymed: DWORD,
}

#[repr(C)]
pub struct STGMEDIUM {
    pub tymed: DWORD,
    pub hGlobal: HANDLE,
    pub pUnkForRelease: *mut c_void,
}

// ------------------------------------------------------------------ 常量

pub const WM_NULL: UINT = 0x0000;
pub const WM_CREATE: UINT = 0x0001;
pub const WM_DESTROY: UINT = 0x0002;
pub const WM_MOVE: UINT = 0x0003;
pub const WM_SIZE: UINT = 0x0005;
pub const WM_ACTIVATE: UINT = 0x0006;
pub const WM_PAINT: UINT = 0x000F;
pub const WM_CLOSE: UINT = 0x0010;
pub const WM_QUIT: UINT = 0x0012;
pub const WM_ERASEBKGND: UINT = 0x0014;
pub const WM_SHOWWINDOW: UINT = 0x0018;
pub const WM_SETTINGCHANGE: UINT = 0x001A;
pub const WM_MOUSEACTIVATE: UINT = 0x0021;
pub const WM_SETCURSOR: UINT = 0x0020;
pub const WM_CONTEXTMENU: UINT = 0x007B;
pub const WM_DISPLAYCHANGE: UINT = 0x007E;
pub const WM_NCCREATE: UINT = 0x0081;
pub const WM_NCDESTROY: UINT = 0x0082;
pub const WM_COMMAND: UINT = 0x0111;
pub const WM_TIMER: UINT = 0x0113;
pub const WM_HOTKEY: UINT = 0x0312;
pub const WM_MOUSEMOVE: UINT = 0x0200;
pub const WM_MOUSEWHEEL: UINT = 0x020A;
pub const WM_DROPFILES: UINT = 0x0233;
pub const WM_KEYDOWN: UINT = 0x0100;
pub const WM_CHAR: UINT = 0x0102;
pub const WM_CTLCOLOREDIT: UINT = 0x0133;
pub const EN_CHANGE: UINT = 0x0300;
pub const ES_AUTOHSCROLL: DWORD = 0x0080;
pub const VK_ESCAPE: WPARAM = 0x1B;
pub const GWLP_WNDPROC: i32 = -4;
pub const WM_LBUTTONDOWN: UINT = 0x0201;
pub const WM_LBUTTONUP: UINT = 0x0202;
pub const WM_LBUTTONDBLCLK: UINT = 0x0203;
pub const WM_RBUTTONDOWN: UINT = 0x0204;
pub const WM_RBUTTONUP: UINT = 0x0205;
pub const WM_CAPTURECHANGED: UINT = 0x0215;
pub const WM_MOUSELEAVE: UINT = 0x02A3;
pub const WM_DPICHANGED: UINT = 0x02E0;
pub const WM_APP: UINT = 0x8000;

pub const WS_POPUP: DWORD = 0x8000_0000;
pub const WS_CHILD: DWORD = 0x4000_0000;
pub const WS_MINIMIZE: DWORD = 0x2000_0000;
pub const WS_VISIBLE: DWORD = 0x1000_0000;
pub const WS_CLIPCHILDREN: DWORD = 0x0200_0000;
pub const WS_CAPTION: DWORD = 0x00C0_0000;
pub const WS_SYSMENU: DWORD = 0x0008_0000;
pub const WS_TABSTOP: DWORD = 0x0001_0000;
pub const WS_GROUP: DWORD = 0x0002_0000;
pub const BS_AUTORADIOBUTTON: DWORD = 0x0000_0009;
pub const BS_AUTOCHECKBOX: DWORD = 0x0000_0003;
pub const BM_GETCHECK: UINT = 0x00F0;
pub const BM_SETCHECK: UINT = 0x00F1;
pub const BST_CHECKED: WPARAM = 1;
pub const BST_UNCHECKED: WPARAM = 0;
pub const DEFAULT_GUI_FONT: i32 = 17;
pub const COLOR_BTNFACE: i32 = 15;

pub const WS_EX_TOPMOST: DWORD = 0x0000_0008;
pub const WS_EX_TRANSPARENT: DWORD = 0x0000_0020;
pub const WS_EX_TOOLWINDOW: DWORD = 0x0000_0080;
pub const WS_EX_APPWINDOW: DWORD = 0x0004_0000;
pub const WS_EX_LAYERED: DWORD = 0x0008_0000;
pub const WS_EX_NOACTIVATE: DWORD = 0x0800_0000;

pub const SWP_NOSIZE: UINT = 0x0001;
pub const SWP_NOMOVE: UINT = 0x0002;
pub const SWP_NOZORDER: UINT = 0x0004;
pub const SWP_NOACTIVATE: UINT = 0x0010;
pub const SWP_SHOWWINDOW: UINT = 0x0040;
pub const SWP_HIDEWINDOW: UINT = 0x0080;

pub const HWND_TOP: HWND = 0 as HWND;
pub const HWND_BOTTOM: HWND = 1 as HWND;
pub const HWND_TOPMOST: HWND = -1isize as HWND;
pub const HWND_NOTOPMOST: HWND = -2isize as HWND;
pub const HWND_BROADCAST: HWND = 0xFFFF as HWND;

pub const SW_HIDE: i32 = 0;
pub const SW_SHOWNORMAL: i32 = 1;
pub const SW_SHOWNOACTIVATE: i32 = 4;
pub const SW_SHOW: i32 = 5;

pub const GWL_EXSTYLE: i32 = -20;
pub const GWL_STYLE: i32 = -16;
/// 桌面图标视图：自动排列样式 + 重新布局消息
pub const LVS_AUTOARRANGE: i32 = 0x0100;
pub const LVM_ARRANGE: UINT = 0x1016;
pub const LVA_DEFAULT: usize = 0x0000;
pub const LVA_SNAPTOGRID: usize = 0x0005;

pub const ULW_ALPHA: DWORD = 0x0000_0002;
pub const AC_SRC_OVER: u8 = 0x00;
pub const AC_SRC_ALPHA: u8 = 0x01;

pub const DIB_RGB_COLORS: UINT = 0;
pub const BI_RGB: DWORD = 0;

pub const TRANSPARENT: i32 = 1;
pub const DT_TOP: UINT = 0x0000;
pub const DT_LEFT: UINT = 0x0000;
pub const DT_CENTER: UINT = 0x0001;
pub const DT_RIGHT: UINT = 0x0002;
pub const DT_VCENTER: UINT = 0x0004;
pub const DT_WORDBREAK: UINT = 0x0010;
pub const DT_SINGLELINE: UINT = 0x0020;
pub const DT_CALCRECT: UINT = 0x0400;
pub const DT_NOPREFIX: UINT = 0x0800;
pub const DT_END_ELLIPSIS: UINT = 0x8000;

pub const ANTIALIASED_QUALITY: DWORD = 4;
pub const CLEARTYPE_QUALITY: DWORD = 5;
pub const DEFAULT_CHARSET: DWORD = 1;
pub const FW_NORMAL: i32 = 400;
pub const FW_SEMIBOLD: i32 = 600;
pub const FW_BOLD: i32 = 700;

pub const DI_NORMAL: UINT = 0x0003;
pub const ILD_TRANSPARENT: UINT = 0x0001;
pub const SHIL_EXTRALARGE: i32 = 2;
pub const SHGFI_ICON: UINT = 0x0000_0100;
pub const SHGFI_LARGEICON: UINT = 0x0000_0000;
pub const SHGFI_SYSICONINDEX: UINT = 0x0000_4000;

pub const NIM_ADD: DWORD = 0;
pub const NIM_MODIFY: DWORD = 1;
pub const NIM_DELETE: DWORD = 2;
pub const NIM_SETVERSION: DWORD = 4;
pub const NIF_MESSAGE: UINT = 0x0001;
pub const NIF_ICON: UINT = 0x0002;
pub const NIF_TIP: UINT = 0x0004;
pub const NIF_INFO: UINT = 0x0010;
pub const NIF_SHOWTIP: UINT = 0x0080;
pub const NOTIFYICON_VERSION_4: UINT = 4;
pub const NIN_SELECT: UINT = 0x0400;
pub const NIN_KEYSELECT: UINT = 0x0401;
pub const NIIF_INFO: UINT = 0x0000_0001;
pub const NIIF_WARNING: UINT = 0x0000_0002;

pub const MF_STRING: UINT = 0x0000;
pub const MF_SEPARATOR: UINT = 0x0800;
pub const MF_CHECKED: UINT = 0x0008;
pub const MF_UNCHECKED: UINT = 0x0000;
pub const MF_GRAYED: UINT = 0x0001;
pub const TPM_LEFTALIGN: UINT = 0x0000;
pub const TPM_RIGHTBUTTON: UINT = 0x0002;
pub const TPM_BOTTOMALIGN: UINT = 0x0020;
pub const TPM_NONOTIFY: UINT = 0x0080;
pub const TPM_RETURNCMD: UINT = 0x0100;

pub const MB_OK: UINT = 0x0000_0000;
pub const MB_YESNO: UINT = 0x0000_0004;
pub const MB_ICONINFORMATION: UINT = 0x0000_0040;
pub const MB_ICONWARNING: UINT = 0x0000_0030;
pub const MB_ICONQUESTION: UINT = 0x0000_0020;
pub const MB_SETFOREGROUND: UINT = 0x0001_0000;
pub const MB_YESNOCANCEL: UINT = 0x0000_0003;
pub const IDCANCEL: i32 = 2;
pub const IDYES: i32 = 6;
pub const IDNO: i32 = 7;

pub const EVENT_SYSTEM_FOREGROUND: DWORD = 0x0003;
pub const EVENT_SYSTEM_MINIMIZESTART: DWORD = 0x0016;
pub const EVENT_SYSTEM_MINIMIZEEND: DWORD = 0x0017;
pub const WINEVENT_OUTOFCONTEXT: UINT = 0x0000;
pub const WINEVENT_SKIPOWNPROCESS: UINT = 0x0002;

pub const MONITOR_DEFAULTTONEAREST: DWORD = 2;
pub const SM_XVIRTUALSCREEN: i32 = 76;
pub const SM_YVIRTUALSCREEN: i32 = 77;
pub const SM_CXVIRTUALSCREEN: i32 = 78;
pub const SM_CYVIRTUALSCREEN: i32 = 79;
pub const LOGPIXELSX: i32 = 88;

pub const IDC_ARROW: LPCWSTR = 32512usize as LPCWSTR;
pub const IDC_SIZENWSE: LPCWSTR = 32642usize as LPCWSTR;
pub const IDI_APPLICATION: LPCWSTR = 32512usize as LPCWSTR;

pub const KEY_SET_VALUE: DWORD = 0x0002;
pub const KEY_QUERY_VALUE: DWORD = 0x0001;
pub const REG_SZ: DWORD = 1;
pub const REG_OPTION_NON_VOLATILE: DWORD = 0;
pub const ERROR_SUCCESS: i32 = 0;
pub const ERROR_FILE_NOT_FOUND: i32 = 2;
pub const ERROR_ALREADY_EXISTS: i32 = 183;

pub const MOD_ALT: UINT = 0x0001;
pub const MOD_CONTROL: UINT = 0x0002;
pub const MOD_SHIFT: UINT = 0x0004;
pub const MOD_WIN: UINT = 0x0008;
pub const MOD_NOREPEAT: UINT = 0x4000;
pub const VK_MENU: i32 = 0x12;

pub const TME_LEAVE: DWORD = 0x0000_0002;

pub const GENERIC_READ: DWORD = 0x8000_0000;
pub const FILE_ATTRIBUTE_NORMAL: DWORD = 0x0000_0080;
pub const FILE_LIST_DIRECTORY: DWORD = 0x0001;
pub const FILE_SHARE_READ: DWORD = 0x0000_0001;
pub const FILE_SHARE_WRITE: DWORD = 0x0000_0002;
pub const FILE_SHARE_DELETE: DWORD = 0x0000_0004;
pub const OPEN_EXISTING: DWORD = 3;
pub const FILE_FLAG_BACKUP_SEMANTICS: DWORD = 0x0200_0000;
pub const FILE_FLAG_OVERLAPPED: DWORD = 0x4000_0000;
pub const FILE_NOTIFY_CHANGE_FILE_NAME: DWORD = 0x0000_0001;
pub const FILE_NOTIFY_CHANGE_DIR_NAME: DWORD = 0x0000_0002;
pub const FILE_NOTIFY_CHANGE_SIZE: DWORD = 0x0000_0008;
pub const FILE_NOTIFY_CHANGE_LAST_WRITE: DWORD = 0x0000_0010;
pub const FILE_ACTION_ADDED: DWORD = 1;
pub const FILE_ACTION_MODIFIED: DWORD = 3;
pub const FILE_ACTION_RENAMED_NEW_NAME: DWORD = 5;
pub const MOVEFILE_REPLACE_EXISTING: DWORD = 0x0000_0001;
pub const MOVEFILE_COPY_ALLOWED: DWORD = 0x0000_0002;
pub const MOVEFILE_WRITE_THROUGH: DWORD = 0x0000_0008;
pub const INVALID_FILE_ATTRIBUTES: DWORD = 0xFFFF_FFFF;
pub const FILE_ATTRIBUTE_DIRECTORY: DWORD = 0x0000_0010;
pub const FILE_ATTRIBUTE_HIDDEN: DWORD = 0x0000_0002;
pub const FILE_ATTRIBUTE_SYSTEM: DWORD = 0x0000_0004;
pub const ERROR_SHARING_VIOLATION: i32 = 32;
pub const ERROR_LOCK_VIOLATION: i32 = 33;
pub const ERROR_ACCESS_DENIED: i32 = 5;

pub const FO_DELETE: UINT = 0x0003;
pub const FOF_SILENT: WORD = 0x0004;
pub const FOF_NOCONFIRMATION: WORD = 0x0010;
pub const FOF_ALLOWUNDO: WORD = 0x0040;
pub const FOF_NOERRORUI: WORD = 0x0400;
pub const FOF_NOCONFIRMMKDIR: WORD = 0x0200;

pub const CF_HDROP: u16 = 15;
pub const DVASPECT_CONTENT: DWORD = 1;
pub const TYMED_HGLOBAL: DWORD = 1;
pub const DROPEFFECT_NONE: DWORD = 0;
pub const DROPEFFECT_COPY: DWORD = 1;
pub const DROPEFFECT_MOVE: DWORD = 2;
pub const DROPEFFECT_LINK: DWORD = 4;

pub const RPC_E_CHANGED_MODE: HRESULT = 0x8001_0106u32 as HRESULT;

pub const QUNS_RUNNING_D3D_FULL_SCREEN: i32 = 3;
pub const QUNS_PRESENTATION_MODE: i32 = 4;

// ------------------------------------------------------------------ user32

#[link(name = "user32")]
unsafe extern "system" {
    pub fn RegisterClassExW(lpWndClass: *const WNDCLASSEXW) -> ATOM;
    pub fn CreateWindowExW(
        dwExStyle: DWORD,
        lpClassName: LPCWSTR,
        lpWindowName: LPCWSTR,
        dwStyle: DWORD,
        X: i32,
        Y: i32,
        nWidth: i32,
        nHeight: i32,
        hWndParent: HWND,
        hMenu: HMENU,
        hInstance: HINSTANCE,
        lpParam: LPVOID,
    ) -> HWND;
    pub fn DefWindowProcW(hWnd: HWND, Msg: UINT, wParam: WPARAM, lParam: LPARAM) -> LRESULT;
    pub fn DestroyWindow(hWnd: HWND) -> BOOL;
    pub fn ShowWindow(hWnd: HWND, nCmdShow: i32) -> BOOL;
    pub fn GetMessageW(lpMsg: *mut MSG, hWnd: HWND, wMsgFilterMin: UINT, wMsgFilterMax: UINT) -> BOOL;
    pub fn TranslateMessage(lpMsg: *const MSG) -> BOOL;
    pub fn DispatchMessageW(lpMsg: *const MSG) -> LRESULT;
    pub fn PostQuitMessage(nExitCode: i32);
    pub fn PostMessageW(hWnd: HWND, Msg: UINT, wParam: WPARAM, lParam: LPARAM) -> BOOL;
    pub fn SendMessageW(hWnd: HWND, Msg: UINT, wParam: WPARAM, lParam: LPARAM) -> LRESULT;
    pub fn SetWindowPos(
        hWnd: HWND,
        hWndInsertAfter: HWND,
        X: i32,
        Y: i32,
        cx: i32,
        cy: i32,
        uFlags: UINT,
    ) -> BOOL;
    pub fn GetWindowRect(hWnd: HWND, lpRect: *mut RECT) -> BOOL;
    pub fn SetCapture(hWnd: HWND) -> HWND;
    pub fn ReleaseCapture() -> BOOL;
    pub fn GetCursorPos(lpPoint: *mut POINT) -> BOOL;
    pub fn GetKeyState(nVirtKey: i32) -> i16;
    pub fn SetTimer(hWnd: HWND, nIDEvent: UINT_PTR, uElapse: UINT, lpTimerFunc: *mut c_void)
        -> UINT_PTR;
    pub fn KillTimer(hWnd: HWND, uIDEvent: UINT_PTR) -> BOOL;
    pub fn LoadCursorW(hInstance: HINSTANCE, lpCursorName: LPCWSTR) -> HCURSOR;
    pub fn LoadIconW(hInstance: HINSTANCE, lpIconName: LPCWSTR) -> HICON;
    pub fn DestroyIcon(hIcon: HICON) -> BOOL;
    pub fn SetCursor(hCursor: HCURSOR) -> HCURSOR;
    pub fn SetForegroundWindow(hWnd: HWND) -> BOOL;
    pub fn CreatePopupMenu() -> HMENU;
    pub fn AppendMenuW(hMenu: HMENU, uFlags: UINT, uIDNewItem: usize, lpNewItem: LPCWSTR) -> BOOL;
    pub fn TrackPopupMenu(
        hMenu: HMENU,
        uFlags: UINT,
        x: i32,
        y: i32,
        nReserved: i32,
        hWnd: HWND,
        prcRect: *const RECT,
    ) -> BOOL;
    pub fn DestroyMenu(hMenu: HMENU) -> BOOL;
    pub fn CheckMenuItem(hMenu: HMENU, uIDCheckItem: UINT, uCheck: UINT) -> DWORD;
    pub fn FindWindowW(lpClassName: LPCWSTR, lpWindowName: LPCWSTR) -> HWND;
    pub fn FindWindowExW(
        hWndParent: HWND,
        hWndChildAfter: HWND,
        lpszClass: LPCWSTR,
        lpszWindow: LPCWSTR,
    ) -> HWND;
    pub fn IsWindowVisible(hWnd: HWND) -> BOOL;
    pub fn SetWinEventHook(
        eventMin: DWORD,
        eventMax: DWORD,
        hmodWinEventProc: HMODULE,
        pfnWinEventProc: WINEVENTPROC,
        idProcess: DWORD,
        idThread: DWORD,
        dwFlags: UINT,
    ) -> HANDLE;
    pub fn UnhookWinEvent(hWinEventHook: HANDLE) -> BOOL;
    pub fn GetSystemMetrics(nIndex: i32) -> i32;
    pub fn MonitorFromRect(lprc: *const RECT, dwFlags: DWORD) -> HMONITOR;
    pub fn MonitorFromWindow(hwnd: HWND, dwFlags: DWORD) -> HMONITOR;
    pub fn GetMonitorInfoW(hMonitor: HMONITOR, lpmi: *mut MONITORINFO) -> BOOL;
    pub fn MessageBoxW(hWnd: HWND, lpText: LPCWSTR, lpCaption: LPCWSTR, uType: UINT) -> i32;
    pub fn RegisterHotKey(hWnd: HWND, id: i32, fsModifiers: UINT, vk: UINT) -> BOOL;
    pub fn UnregisterHotKey(hWnd: HWND, id: i32) -> BOOL;
    pub fn GetDC(hWnd: HWND) -> HDC;
    pub fn ReleaseDC(hWnd: HWND, hDC: HDC) -> i32;
    pub fn TrackMouseEvent(lpEventTrack: *mut TRACKMOUSEEVENT) -> BOOL;
    pub fn GetGuiResources(hProcess: HANDLE, uiFlags: DWORD) -> DWORD;
    pub fn GetDoubleClickTime() -> UINT;
    pub fn GetMessageTime() -> LONG;
    pub fn GetWindowLongW(hWnd: HWND, nIndex: i32) -> i32;
    pub fn SetWindowLongW(hWnd: HWND, nIndex: i32, dwNewLong: i32) -> i32;
    pub fn SetWindowLongPtrW(hWnd: HWND, nIndex: i32, dwNewLong: isize) -> isize;
    pub fn CallWindowProcW(
        lpPrevWndFunc: isize,
        hWnd: HWND,
        Msg: UINT,
        wParam: WPARAM,
        lParam: LPARAM,
    ) -> LRESULT;
    pub fn GetWindowTextW(hWnd: HWND, lpString: LPWSTR, nMaxCount: i32) -> i32;
    pub fn SetWindowTextW(hWnd: HWND, lpString: LPCWSTR) -> BOOL;
    pub fn SetFocus(hWnd: HWND) -> HWND;
    pub fn GetFocus() -> HWND;
    pub fn GetStockObject(i: i32) -> HGDIOBJ;
    pub fn GetParent(hWnd: HWND) -> HWND;
    pub fn UpdateLayeredWindow(
        hWnd: HWND,
        hdcDst: HDC,
        pptDst: *const POINT,
        psize: *const SIZE,
        hdcSrc: HDC,
        pptSrc: *const POINT,
        crKey: DWORD,
        pblend: *const BLENDFUNCTION,
        dwFlags: DWORD,
    ) -> BOOL;
    pub fn SetProcessDPIAware() -> BOOL;
    pub fn RegisterWindowMessageW(lpString: LPCWSTR) -> UINT;
    pub fn ChangeWindowMessageFilterEx(hwnd: HWND, msg: UINT, action: DWORD, pcs: *mut c_void)
        -> BOOL;
    pub fn DrawIconEx(
        hdc: HDC,
        xLeft: i32,
        yTop: i32,
        hIcon: HICON,
        cxWidth: i32,
        cyWidth: i32,
        istepIfAniCur: UINT,
        hbrFlickerFreeDraw: HBRUSH,
        diFlags: UINT,
    ) -> BOOL;
}

// ------------------------------------------------------------------ gdi32

#[link(name = "gdi32")]
unsafe extern "system" {
    pub fn CreateCompatibleDC(hdc: HDC) -> HDC;
    pub fn CreateDIBSection(
        hdc: HDC,
        pbmi: *const BITMAPINFO,
        usage: UINT,
        ppvBits: *mut *mut c_void,
        hSection: HANDLE,
        offset: DWORD,
    ) -> HBITMAP;
    pub fn SelectObject(hdc: HDC, h: HGDIOBJ) -> HGDIOBJ;
    pub fn DeleteObject(h: HGDIOBJ) -> BOOL;
    pub fn DeleteDC(hdc: HDC) -> BOOL;
    pub fn CreateFontW(
        cHeight: i32,
        cWidth: i32,
        cEscapement: i32,
        cOrientation: i32,
        cWeight: i32,
        bItalic: DWORD,
        bUnderline: DWORD,
        bStrikeOut: DWORD,
        iCharSet: DWORD,
        iOutPrecision: DWORD,
        iClipPrecision: DWORD,
        iQuality: DWORD,
        iPitchAndFamily: DWORD,
        pszFaceName: LPCWSTR,
    ) -> HFONT;
    pub fn DrawTextW(hdc: HDC, lpchText: LPCWSTR, cchText: i32, lprc: *mut RECT, format: UINT)
        -> i32;
    pub fn SetBkMode(hdc: HDC, mode: i32) -> i32;
    pub fn SetTextColor(hdc: HDC, color: DWORD) -> DWORD;
    pub fn SetBkColor(hdc: HDC, color: DWORD) -> DWORD;
    pub fn CreateSolidBrush(color: DWORD) -> HBRUSH;
    pub fn GetTextExtentPoint32W(hdc: HDC, lpString: LPCWSTR, c: i32, psizl: *mut SIZE) -> BOOL;
    pub fn BitBlt(
        hdc: HDC,
        x: i32,
        y: i32,
        cx: i32,
        cy: i32,
        hdcSrc: HDC,
        x1: i32,
        y1: i32,
        rop: DWORD,
    ) -> BOOL;
    pub fn GetDeviceCaps(hdc: HDC, index: i32) -> i32;
    pub fn GetObjectW(h: HGDIOBJ, c: i32, pv: *mut c_void) -> i32;
    pub fn GetDIBits(
        hdc: HDC,
        hbm: HBITMAP,
        start: UINT,
        cLines: UINT,
        lpvBits: *mut c_void,
        lpbmi: *mut BITMAPINFO,
        usage: UINT,
    ) -> i32;
    pub fn GdiFlush() -> BOOL;
}

// ------------------------------------------------------------------ kernel32

#[link(name = "kernel32")]
unsafe extern "system" {
    pub fn GetModuleHandleW(lpModuleName: LPCWSTR) -> HMODULE;
    pub fn GetModuleFileNameW(hModule: HMODULE, lpFilename: LPWSTR, nSize: DWORD) -> DWORD;
    pub fn CreateMutexW(lpMutexAttributes: *mut c_void, bInitialOwner: BOOL, lpName: LPCWSTR)
        -> HANDLE;
    pub fn GetLastError() -> DWORD;
    pub fn SetLastError(dwErrCode: DWORD);
    pub fn CloseHandle(hObject: HANDLE) -> BOOL;
    pub fn GetTickCount64() -> u64;
    pub fn FileTimeToLocalFileTime(lpFileTime: *const FILETIME, lpLocalFileTime: *mut FILETIME) -> BOOL;
    pub fn FileTimeToSystemTime(lpFileTime: *const FILETIME, lpSystemTime: *mut SYSTEMTIME) -> BOOL;
    pub fn GetLocalTime(lpSystemTime: *mut SYSTEMTIME);
    pub fn Sleep(dwMilliseconds: DWORD);
    pub fn GetProcAddress(hModule: HMODULE, lpProcName: LPCSTR) -> *mut c_void;
    pub fn CreateEventW(
        lpEventAttributes: *mut c_void,
        bManualReset: BOOL,
        bInitialState: BOOL,
        lpName: LPCWSTR,
    ) -> HANDLE;
    pub fn SetEvent(hEvent: HANDLE) -> BOOL;
    pub fn ResetEvent(hEvent: HANDLE) -> BOOL;
    pub fn WaitForMultipleObjects(
        nCount: DWORD,
        lpHandles: *const HANDLE,
        bWaitAll: BOOL,
        dwMilliseconds: DWORD,
    ) -> DWORD;
    pub fn CreateFileW(
        lpFileName: LPCWSTR,
        dwDesiredAccess: DWORD,
        dwShareMode: DWORD,
        lpSecurityAttributes: *mut c_void,
        dwCreationDisposition: DWORD,
        dwFlagsAndAttributes: DWORD,
        hTemplateFile: HANDLE,
    ) -> HANDLE;
    pub fn ReadDirectoryChangesW(
        hDirectory: HANDLE,
        lpBuffer: *mut c_void,
        nBufferLength: DWORD,
        bWatchSubtree: BOOL,
        dwNotifyFilter: DWORD,
        lpBytesReturned: *mut DWORD,
        lpOverlapped: *mut OVERLAPPED,
        lpCompletionRoutine: *mut c_void,
    ) -> BOOL;
    pub fn GetOverlappedResult(
        hFile: HANDLE,
        lpOverlapped: *mut OVERLAPPED,
        lpNumberOfBytesTransferred: *mut DWORD,
        bWait: BOOL,
    ) -> BOOL;
    pub fn CancelIoEx(hFile: HANDLE, lpOverlapped: *mut OVERLAPPED) -> BOOL;
    pub fn MoveFileExW(lpExistingFileName: LPCWSTR, lpNewFileName: LPCWSTR, dwFlags: DWORD) -> BOOL;
    pub fn GetFileAttributesW(lpFileName: LPCWSTR) -> DWORD;
    pub fn FindFirstFileW(lpFileName: LPCWSTR, lpFindFileData: *mut WIN32_FIND_DATAW) -> HANDLE;
    pub fn FindNextFileW(hFindFile: HANDLE, lpFindFileData: *mut WIN32_FIND_DATAW) -> BOOL;
    pub fn FindClose(hFindFile: HANDLE) -> BOOL;
    pub fn CreateProcessW(
        lpApplicationName: LPCWSTR,
        lpCommandLine: LPWSTR,
        lpProcessAttributes: *mut c_void,
        lpThreadAttributes: *mut c_void,
        bInheritHandles: BOOL,
        dwCreationFlags: DWORD,
        lpEnvironment: *mut c_void,
        lpCurrentDirectory: LPCWSTR,
        lpStartupInfo: *mut c_void,
        lpProcessInformation: *mut c_void,
    ) -> BOOL;
    pub fn GetCurrentProcess() -> HANDLE;
}

// ------------------------------------------------------------------ advapi32

#[link(name = "advapi32")]
unsafe extern "system" {
    pub fn RegCreateKeyExW(
        hKey: HKEY,
        lpSubKey: LPCWSTR,
        Reserved: DWORD,
        lpClass: LPWSTR,
        dwOptions: DWORD,
        samDesired: DWORD,
        lpSecurityAttributes: *mut c_void,
        phkResult: *mut HKEY,
        lpdwDisposition: *mut DWORD,
    ) -> LONG;
    pub fn RegSetValueExW(
        hKey: HKEY,
        lpValueName: LPCWSTR,
        Reserved: DWORD,
        dwType: DWORD,
        lpData: *const u8,
        cbData: DWORD,
    ) -> LONG;
    pub fn RegQueryValueExW(
        hKey: HKEY,
        lpValueName: LPCWSTR,
        lpReserved: *mut DWORD,
        lpType: *mut DWORD,
        lpData: *mut u8,
        lpcbData: *mut DWORD,
    ) -> LONG;
    pub fn RegCloseKey(hKey: HKEY) -> LONG;
    pub fn RegDeleteValueW(hKey: HKEY, lpValueName: LPCWSTR) -> LONG;
}

// ------------------------------------------------------------------ shell32

#[link(name = "shell32")]
unsafe extern "system" {
    pub fn Shell_NotifyIconW(dwMessage: DWORD, lpData: *const NOTIFYICONDATAW) -> BOOL;
    pub fn ShellExecuteW(
        hwnd: HWND,
        lpOperation: LPCWSTR,
        lpFile: LPCWSTR,
        lpParameters: LPCWSTR,
        lpDirectory: LPCWSTR,
        nShowCmd: i32,
    ) -> HINSTANCE;
    pub fn SHFileOperationW(lpFileOp: *const SHFILEOPSTRUCTW) -> i32;
    pub fn SHGetFileInfoW(
        pszPath: LPCWSTR,
        dwFileAttributes: DWORD,
        psfi: *mut SHFILEINFOW,
        cbFileInfo: UINT,
        uFlags: UINT,
    ) -> usize;
    pub fn SHGetKnownFolderPath(
        rfid: *const GUID,
        dwFlags: DWORD,
        hToken: HANDLE,
        ppszPath: *mut *mut u16,
    ) -> HRESULT;
    pub fn DragQueryFileW(hDrop: HANDLE, iFile: UINT, lpszFile: LPWSTR, cch: UINT) -> UINT;
    pub fn DragFinish(hDrop: HANDLE);
    pub fn DragAcceptFiles(hWnd: HWND, fAccept: BOOL);
    pub fn SHQueryUserNotificationState(pquns: *mut i32) -> HRESULT;
    pub fn SHCreateItemFromParsingName(
        pszPath: LPCWSTR,
        pbc: *mut c_void,
        riid: *const GUID,
        ppv: *mut *mut c_void,
    ) -> HRESULT;
}

// ------------------------------------------------------------------ ole32

#[link(name = "ole32")]
unsafe extern "system" {
    pub fn OleInitialize(pvReserved: *mut c_void) -> HRESULT;
    pub fn OleUninitialize();
    pub fn CoTaskMemFree(pv: *mut c_void);
    pub fn RegisterDragDrop(hwnd: HWND, pDropTarget: *mut c_void) -> HRESULT;
    pub fn RevokeDragDrop(hwnd: HWND) -> HRESULT;
    pub fn ReleaseStgMedium(pmedium: *mut STGMEDIUM);
    pub fn DoDragDrop(
        pDataObj: *mut c_void,
        pDropSource: *mut c_void,
        dwOKEffects: DWORD,
        pdwEffect: *mut DWORD,
    ) -> HRESULT;
}

// ------------------------------------------------------------------ 工具函数

/// Rust 字符串 → 带结尾 NUL 的 UTF-16。
pub fn w(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

/// 读一个以 NUL 结尾的宽字符串。
pub unsafe fn from_wide(p: *const u16) -> String {
    if p.is_null() {
        return String::new();
    }
    let mut len = 0usize;
    while *p.add(len) != 0 {
        len += 1;
        if len > 4096 {
            break;
        }
    }
    String::from_utf16_lossy(std::slice::from_raw_parts(p, len))
}

/// 把字符串写进定长宽字符数组，截断并补 NUL。
pub fn fill_wide(dst: &mut [u16], s: &str) {
    if dst.is_empty() {
        return;
    }
    let mut i = 0;
    for c in s.encode_utf16() {
        if i + 1 >= dst.len() {
            break;
        }
        dst[i] = c;
        i += 1;
    }
    dst[i] = 0;
}

/// 取一个模块里的函数指针，失败返回 None。
pub unsafe fn proc(modname: &str, name: &str) -> Option<*mut c_void> {
    let hm = GetModuleHandleW(w(modname).as_ptr());
    if hm.is_null() {
        return None;
    }
    let mut nm = Vec::with_capacity(name.len() + 1);
    nm.extend_from_slice(name.as_bytes());
    nm.push(0);
    let p = GetProcAddress(hm, nm.as_ptr());
    if p.is_null() {
        None
    } else {
        Some(p)
    }
}

/// 按序号取模块导出（`SHGetImageList` 只有序号）。
pub unsafe fn proc_ordinal(modname: &str, ord: usize) -> Option<*mut c_void> {
    let hm = GetModuleHandleW(w(modname).as_ptr());
    if hm.is_null() {
        return None;
    }
    let p = GetProcAddress(hm, ord as *const u8);
    if p.is_null() {
        None
    } else {
        Some(p)
    }
}
