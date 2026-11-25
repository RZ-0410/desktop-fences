//! 围栏属性对话框：名称、**存放目录**、**写入围栏的文件类型**、列数。
//!
//! 新建围栏和编辑围栏共用同一个对话框：`show()` 传入初始值，返回用户确认后的值。
//! 目录用系统的 SHBrowseForFolderW 选择，不自己画目录树。

use std::ffi::c_void;
use std::ptr::null_mut;

use crate::winapi::*;

pub const CLASS_NAME: &str = "DFFenceDlg";

const ID_NAME: usize = 3001;
const ID_DIR: usize = 3002;
const ID_BROWSE: usize = 3003;
const ID_TYPES: usize = 3004;
const ID_COLS: usize = 3005;
const ID_OK: usize = 3006;
const ID_CANCEL: usize = 3007;

/// 一个围栏的可编辑属性。
#[derive(Clone, Debug)]
pub struct FenceProps {
    pub name: String,
    pub dir: std::path::PathBuf,
    /// 逗号分隔的扩展名，如 ["doc","pdf","png"]
    pub rules: Vec<String>,
    pub cols: i32,
}

struct Dlg {
    hwnd: HWND,
    name: HWND,
    dir: HWND,
    types: HWND,
    cols: HWND,
    hinst: HINSTANCE,
    owner: HWND,
    result: Option<FenceProps>,
}

static mut DLG: *mut Dlg = std::ptr::null_mut();

fn dlg() -> Option<&'static mut Dlg> {
    unsafe {
        if DLG.is_null() {
            None
        } else {
            Some(&mut *DLG)
        }
    }
}

/// 打开对话框；用户点确定返回 Some，取消/关闭返回 None。
pub unsafe fn show(hinst: HINSTANCE, owner: HWND, initial: FenceProps) -> Option<FenceProps> {
    if !DLG.is_null() {
        // 已经开着，不重复弹
        return None;
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

    let (ww, hh) = (480, 330);
    let hwnd = CreateWindowExW(
        0,
        cls.as_ptr(),
        w("围栏属性").as_ptr(),
        WS_POPUP | WS_CAPTION | WS_SYSMENU,
        240,
        160,
        ww,
        hh,
        owner,
        null_mut(),
        hinst,
        null_mut(),
    );
    if hwnd.is_null() {
        return None;
    }

    let font = GetStockObject(DEFAULT_GUI_FONT);
    let mut mk = |cls: &str, text: &str, id: usize, x: i32, y: i32, bw: i32, bh: i32, style: DWORD| {
        let h = CreateWindowExW(
            0,
            w(cls).as_ptr(),
            w(text).as_ptr(),
            WS_CHILD | WS_VISIBLE | style,
            x,
            y,
            bw,
            bh,
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

    // 标签
    mk("Static", "名称：", 0, 20, 22, 135, 20, 0);
    mk("Static", "存放目录：", 0, 20, 58, 135, 20, 0);
    mk("Static", "写入的文件类型：", 0, 20, 94, 135, 20, 0);
    mk("Static", "（逗号分隔；留空表示不自动收纳）", 0, 20, 132, 300, 18, 0);
    mk("Static", "每行图标数：", 0, 20, 178, 135, 20, 0);

    let st = Box::new(Dlg {
        hwnd,
        name: mk("Edit", &initial.name, ID_NAME, 138, 20, 292, 24, WS_TABSTOP),
        dir: mk(
            "Edit",
            &initial.dir.to_string_lossy(),
            ID_DIR,
            138,
            56,
            224,
            24,
            WS_TABSTOP | ES_AUTOHSCROLL,
        ),
        types: mk(
            "Edit",
            &initial.rules.join(","),
            ID_TYPES,
            20,
            112,
            410,
            24,
            WS_TABSTOP | ES_AUTOHSCROLL,
        ),
        cols: mk("Edit", &initial.cols.to_string(), ID_COLS, 138, 176, 60, 24, WS_TABSTOP),
        hinst,
        owner,
        result: None,
    });
    DLG = Box::into_raw(st);

    mk("Button", "浏览…", ID_BROWSE, 370, 56, 60, 24, WS_TABSTOP);
    mk("Button", "确定", ID_OK, 260, 240, 84, 30, WS_TABSTOP);
    mk("Button", "取消", ID_CANCEL, 352, 240, 84, 30, WS_TABSTOP);

    ShowWindow(hwnd, SW_SHOW);
    SetForegroundWindow(hwnd);

    // 模态：跑自己的消息循环，父窗口不响应
    let mut msg: MSG = std::mem::zeroed();
    while GetMessageW(&mut msg, null_mut(), 0, 0) > 0 {
        TranslateMessage(&msg);
        DispatchMessageW(&msg);
    }

    let out = unsafe {
        if DLG.is_null() {
            None
        } else {
            let d = Box::from_raw(DLG);
            DLG = std::ptr::null_mut();
            d.result
        }
    };
    out
}

/// 读取目录选择结果。
unsafe fn browse_dir(hwnd_owner: HWND) -> Option<std::path::PathBuf> {
    let mut display = vec![0u16; 260];
    // 标题必须绑定到变量：临时 Vec 在 let 语句结束就释放，指针会悬垂
    let title = w("选择围栏文件的存放目录");
    let bi = BROWSEINFOW {
        hwndOwner: hwnd_owner,
        pidlRoot: null_mut(),
        pszDisplayName: display.as_mut_ptr(),
        lpszTitle: title.as_ptr(),
        ulFlags: BIF_RETURNONLYFSDIRS,
        lpfn: null_mut(),
        lParam: 0,
        iImage: 0,
    };
    let pidl = SHBrowseForFolderW(&bi as *const BROWSEINFOW);
    if pidl.is_null() {
        return None;
    }
    let mut path = vec![0u16; 260];
    let ok = SHGetPathFromIDListW(pidl, path.as_mut_ptr());
    CoTaskMemFree(pidl as *mut c_void);
    if ok == 0 {
        return None;
    }
    let s = String::from_utf16_lossy(&path[..path.iter().position(|&c| c == 0).unwrap_or(260)]);
    if s.is_empty() {
        None
    } else {
        Some(std::path::PathBuf::from(s))
    }
}

unsafe fn read_fields() -> Option<FenceProps> {
    let d = dlg()?;
    let get = |h: HWND| -> String {
        let mut buf = vec![0u16; 512];
        let n = GetWindowTextW(h, buf.as_mut_ptr(), buf.len() as i32);
        String::from_utf16_lossy(&buf[..n.max(0) as usize]).trim().to_string()
    };
    let name = get(d.name);
    let dir_s = get(d.dir);
    let types_s = get(d.types);
    let cols_s = get(d.cols);

    if name.is_empty() {
        return None;
    }
    if dir_s.is_empty() {
        return None;
    }
    let cols: i32 = cols_s.parse().unwrap_or(4).clamp(1, 20);
    let rules: Vec<String> = types_s
        .split(|c| c == ',' || c == '，' || c == ' ')
        .map(|x| x.trim().trim_start_matches('.').to_ascii_lowercase())
        .filter(|x| !x.is_empty())
        .collect();
    Some(FenceProps {
        name,
        dir: std::path::PathBuf::from(dir_s),
        rules,
        cols,
    })
}

unsafe extern "system" fn proc(hwnd: HWND, msg: UINT, wp: WPARAM, lp: LPARAM) -> LRESULT {
    match msg {
        WM_COMMAND => {
            let id = wp & 0xffff;
            match id {
                x if x == ID_BROWSE => {
                    if let Some(p) = browse_dir(hwnd) {
                        if let Some(d) = dlg() {
                            SetWindowTextW(d.dir, w(&p.to_string_lossy()).as_ptr());
                        }
                    }
                    return 0;
                }
                x if x == ID_OK => {
                    if let Some(props) = read_fields() {
                        if let Some(d) = dlg() {
                            d.result = Some(props);
                        }
                        DestroyWindow(hwnd);
                        PostQuitMessage(0);
                    } else {
                        MessageBoxW(
                            hwnd,
                            w("请填写名称与存放目录。").as_ptr(),
                            w("围栏属性").as_ptr(),
                            MB_OK | MB_ICONWARNING,
                        );
                    }
                    return 0;
                }
                x if x == ID_CANCEL => {
                    DestroyWindow(hwnd);
                    PostQuitMessage(0);
                    return 0;
                }
                _ => {}
            }
            return 0;
        }
        WM_CLOSE => {
            DestroyWindow(hwnd);
            PostQuitMessage(0);
            return 0;
        }
        _ => {}
    }
    DefWindowProcW(hwnd, msg, wp, lp)
}
