//! OLE 拖放：从资源管理器把文件拖进围栏。
//!
//! 手写 `IDropTarget` 的 7 槽 vtable。几个容易错的地方：
//! - `DragEnter/DragOver/Drop` 的坐标参数是 **POINTL**（两个 LONG），不是 POINT；
//! - `GetData` 要的是 `FORMATETC{cfFormat:15 /*CF_HDROP*/, lindex:-1, tymed:1}`；
//! - 用完必须 `ReleaseStgMedium`，否则 Explorer 侧会泄漏；
//! - Drop 里不要做长任务 —— 这里直接搬文件是为了先跑通，跨盘大文件会短暂卡住 UI。

use std::ffi::c_void;
use std::mem::zeroed;
use std::path::{Path, PathBuf};
use std::ptr::null_mut;

use crate::winapi::*;

const IID_IUNKNOWN: GUID = GUID {
    data1: 0x0000_0000,
    data2: 0x0000,
    data3: 0x0000,
    data4: [0xC0, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x46],
};

const IID_IDROPTARGET: GUID = GUID {
    data1: 0x0000_0122,
    data2: 0x0000,
    data3: 0x0000,
    data4: [0xC0, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x46],
};

#[repr(C)]
pub struct DropTarget {
    vtbl: *const Vtbl,
    refs: u32,
    hwnd: HWND,
}

#[repr(C)]
struct Vtbl {
    query_interface:
        unsafe extern "system" fn(*mut c_void, *const GUID, *mut *mut c_void) -> HRESULT,
    add_ref: unsafe extern "system" fn(*mut c_void) -> u32,
    release: unsafe extern "system" fn(*mut c_void) -> u32,
    drag_enter:
        unsafe extern "system" fn(*mut c_void, *mut c_void, u32, POINTL, *mut u32) -> HRESULT,
    drag_over: unsafe extern "system" fn(*mut c_void, u32, POINTL, *mut u32) -> HRESULT,
    drag_leave: unsafe extern "system" fn(*mut c_void) -> HRESULT,
    com_drop: unsafe extern "system" fn(*mut c_void, *mut c_void, u32, POINTL, *mut u32) -> HRESULT,
}

static VTBL: Vtbl = Vtbl {
    query_interface,
    add_ref,
    release,
    drag_enter,
    drag_over,
    drag_leave,
    com_drop: drop_files,
};

fn guid_eq(a: *const GUID, b: &GUID) -> bool {
    if a.is_null() {
        return false;
    }
    unsafe {
        (*a).data1 == b.data1
            && (*a).data2 == b.data2
            && (*a).data3 == b.data3
            && (*a).data4 == b.data4
    }
}

fn hdrop_format() -> FORMATETC {
    FORMATETC {
        cfFormat: CF_HDROP,
        ptd: null_mut(),
        dwAspect: DVASPECT_CONTENT,
        lindex: -1,
        tymed: TYMED_HGLOBAL,
    }
}

/// 在围栏窗口上注册拖放。成功后我们自己的初始引用要释放掉，
/// 由 OLE 持有的那份引用负责生命周期。
pub unsafe fn register(hwnd: HWND) -> bool {
    let target = Box::into_raw(Box::new(DropTarget { vtbl: &VTBL, refs: 1, hwnd }));
    let hr = RegisterDragDrop(hwnd, target as *mut c_void);
    crate::dlog(&format!("RegisterDragDrop hwnd={:?} hr=0x{:08X}", hwnd, hr));
    release(target as *mut c_void);
    hr == S_OK
}

/// `WM_DROPFILES` 兜底路径：OLE 注册失败时仍然能收文件。
/// 这条路径拿不到拖动中的悬停事件，但足以完成搬运。
pub unsafe fn handle_dropfiles(hwnd: HWND, hdrop: HANDLE) {
    let count = DragQueryFileW(hdrop, 0xFFFF_FFFF, null_mut(), 0) as usize;
    let mut paths = Vec::with_capacity(count);
    for i in 0..count {
        let len = DragQueryFileW(hdrop, i as u32, null_mut(), 0) as usize;
        if len == 0 {
            continue;
        }
        let mut buf = vec![0u16; len + 1];
        let n = DragQueryFileW(hdrop, i as u32, buf.as_mut_ptr(), (len + 1) as u32) as usize;
        if n > 0 {
            paths.push(PathBuf::from(String::from_utf16_lossy(&buf[..n])));
        }
    }
    DragFinish(hdrop);
    crate::dlog(&format!("WM_DROPFILES hwnd={:?} count={}", hwnd, paths.len()));
    apply_drop(hwnd, paths, crate::winapi::GetKeyState(0x11) < 0);
}

/// 统一搬运入口，OLE 与 WM_DROPFILES 共用。
pub unsafe fn apply_drop(hwnd: HWND, paths: Vec<PathBuf>, copy: bool) {
    let dir = match crate::fence_mut(hwnd) {
        Some(f) => f.cfg.dir.clone(),
        None => return,
    };
    if std::fs::create_dir_all(&dir).is_err() {
        return;
    }
    let mut moved = 0usize;
    let mut failed: Vec<String> = Vec::new();
    let batch = crate::journal::next_batch();
    for src in &paths {
        match transfer(src, &dir, copy) {
            Ok(target) => {
                moved += 1;
                // 只有「移动」才记日志：复制的话原件还在，不需要还原
                if !copy {
                    let _ = crate::journal::append(crate::journal::Op::Move, batch, src, &target);
                }
            }
            Err(e) => failed.push(format!(
                "{}：{}",
                src.file_name().unwrap_or_default().to_string_lossy(),
                e
            )),
        }
    }
    if let Some(f) = crate::fence_mut(hwnd) {
        f.refresh();
        f.redraw();
    }
    if !failed.is_empty() {
        let msg = format!(
            "{} 个文件已{}，{} 个失败：\n{}",
            moved,
            if copy { "复制" } else { "移入" },
            failed.len(),
            failed.join("\n")
        );
        crate::tray::balloon(crate::app().hctrl, "桌面围栏", &msg);
    }
}

unsafe extern "system" fn query_interface(
    this: *mut c_void,
    riid: *const GUID,
    ppv: *mut *mut c_void,
) -> HRESULT {
    if ppv.is_null() {
        return E_POINTER;
    }
    if guid_eq(riid, &IID_IUNKNOWN) || guid_eq(riid, &IID_IDROPTARGET) {
        add_ref(this);
        *ppv = this;
        S_OK
    } else {
        *ppv = null_mut();
        E_NOINTERFACE
    }
}

unsafe extern "system" fn add_ref(this: *mut c_void) -> u32 {
    let t = &mut *(this as *mut DropTarget);
    t.refs += 1;
    t.refs
}

unsafe extern "system" fn release(this: *mut c_void) -> u32 {
    let t = &mut *(this as *mut DropTarget);
    t.refs = t.refs.saturating_sub(1);
    let left = t.refs;
    if left == 0 {
        std::mem::drop(Box::from_raw(this as *mut DropTarget));
    }
    left
}

unsafe fn has_hdrop(data: *mut c_void) -> bool {
    if data.is_null() {
        return false;
    }
    let vtbl = *(data as *mut *const usize);
    type QueryGetData = unsafe extern "system" fn(*mut c_void, *const FORMATETC) -> HRESULT;
    let q: QueryGetData = std::mem::transmute(*vtbl.add(5));
    let etc = hdrop_format();
    q(data, &etc) == S_OK
}

fn effect_for(keys: u32) -> u32 {
    if keys & MK_CONTROL != 0 {
        DROPEFFECT_COPY
    } else {
        DROPEFFECT_MOVE
    }
}

unsafe fn set_active(hwnd: HWND, on: bool) {
    if let Some(f) = crate::fence_mut(hwnd) {
        if f.drop_active != on {
            f.drop_active = on;
            f.redraw();
        }
    }
}

unsafe extern "system" fn drag_enter(
    this: *mut c_void,
    data: *mut c_void,
    keys: u32,
    _pt: POINTL,
    effect: *mut u32,
) -> HRESULT {
    let t = &*(this as *mut DropTarget);
    crate::dlog(&format!("DragEnter hwnd={:?} has_hdrop={}", t.hwnd, has_hdrop(data)));
    if !has_hdrop(data) {
        if !effect.is_null() {
            *effect = DROPEFFECT_NONE;
        }
        return S_OK;
    }
    if !effect.is_null() {
        *effect = effect_for(keys);
    }
    set_active(t.hwnd, true);
    S_OK
}

unsafe extern "system" fn drag_over(
    _this: *mut c_void,
    keys: u32,
    _pt: POINTL,
    effect: *mut u32,
) -> HRESULT {
    if !effect.is_null() {
        *effect = effect_for(keys);
    }
    S_OK
}

unsafe extern "system" fn drag_leave(this: *mut c_void) -> HRESULT {
    let t = &*(this as *mut DropTarget);
    crate::dlog(&format!("DragLeave hwnd={:?}", t.hwnd));
    set_active(t.hwnd, false);
    S_OK
}

unsafe extern "system" fn drop_files(
    this: *mut c_void,
    data: *mut c_void,
    keys: u32,
    _pt: POINTL,
    effect: *mut u32,
) -> HRESULT {
    let t = &*(this as *mut DropTarget);
    let hwnd = t.hwnd;
    let copy = keys & MK_CONTROL != 0;
    crate::dlog(&format!("Drop hwnd={:?} copy={}", hwnd, copy));

    set_active(hwnd, false);

    if data.is_null() {
        if !effect.is_null() {
            *effect = DROPEFFECT_NONE;
        }
        return S_OK;
    }

    let vtbl = *(data as *mut *const usize);
    type GetData =
        unsafe extern "system" fn(*mut c_void, *const FORMATETC, *mut STGMEDIUM) -> HRESULT;
    let get: GetData = std::mem::transmute(*vtbl.add(3));

    let mut etc = hdrop_format();
    let mut medium: STGMEDIUM = zeroed();
    if get(data, &etc, &mut medium) != S_OK {
        if !effect.is_null() {
            *effect = DROPEFFECT_NONE;
        }
        return S_OK;
    }

    let hdrop = medium.hGlobal;
    let count = DragQueryFileW(hdrop, 0xFFFF_FFFF, null_mut(), 0) as usize;
    let mut paths: Vec<PathBuf> = Vec::with_capacity(count);
    for i in 0..count {
        let len = DragQueryFileW(hdrop, i as u32, null_mut(), 0) as usize;
        if len == 0 {
            continue;
        }
        let mut buf = vec![0u16; len + 1];
        let n = DragQueryFileW(hdrop, i as u32, buf.as_mut_ptr(), (len + 1) as u32) as usize;
        if n > 0 {
            paths.push(PathBuf::from(String::from_utf16_lossy(&buf[..n])));
        }
    }
    ReleaseStgMedium(&mut medium);

    // 目标目录
    let dir = match crate::fence_mut(hwnd) {
        Some(f) => f.cfg.dir.clone(),
        None => {
            if !effect.is_null() {
                *effect = DROPEFFECT_NONE;
            }
            return S_OK;
        }
    };
    if std::fs::create_dir_all(&dir).is_err() {
        if !effect.is_null() {
            *effect = DROPEFFECT_NONE;
        }
        return S_OK;
    }

    let mut moved = 0usize;
    let mut failed: Vec<String> = Vec::new();
    let batch = crate::journal::next_batch();
    for src in &paths {
        match transfer(src, &dir, copy) {
            Ok(target) => {
                moved += 1;
                // 只有「移动」才记日志：复制的话原件还在，不需要还原
                if !copy {
                    let _ = crate::journal::append(crate::journal::Op::Move, batch, src, &target);
                }
            }
            Err(e) => failed.push(format!("{}：{}", src.file_name().unwrap_or_default().to_string_lossy(), e)),
        }
    }

    if let Some(f) = crate::fence_mut(hwnd) {
        f.refresh();
        f.redraw();
    }

    if !failed.is_empty() {
        let msg = format!("{} 个文件已{}，{} 个失败：\n{}", moved, if copy { "复制" } else { "移入" }, failed.len(), failed.join("\n"));
        crate::tray::balloon(crate::app().hctrl, "桌面围栏", &msg);
    }

    if !effect.is_null() {
        *effect = if moved > 0 {
            if copy {
                DROPEFFECT_COPY
            } else {
                DROPEFFECT_MOVE
            }
        } else {
            DROPEFFECT_NONE
        };
    }
    S_OK
}

/// 把单个文件/文件夹搬到围栏目录；重名自动加 (2)。
/// 文件夹只做移动（同盘 rename），**坚决不做递归复制**。
pub fn transfer(src: &Path, dir: &Path, copy: bool) -> Result<PathBuf, String> {
    let name = src
        .file_name()
        .ok_or_else(|| "无法取文件名".to_string())?
        .to_string_lossy()
        .to_string();

    // 已经在目标目录里就别动
    if src.parent().map(|p| same_path(p, dir)).unwrap_or(false) {
        return Ok(src.to_path_buf());
    }

    let target = unique_target(dir, src, &name);
    let is_dir = src.is_dir();

    if is_dir && copy {
        return Err("文件夹不支持复制，只能移动".to_string());
    }

    if copy {
        std::fs::copy(src, &target).map_err(|e| e.to_string())?;
        return Ok(target);
    }

    // 同盘 rename 最快；跨盘失败后退回「复制 + 删源」
    match std::fs::rename(src, &target) {
        Ok(_) => Ok(target),
        Err(_) if is_dir => Err("跨盘移动文件夹不支持".to_string()),
        Err(_) => {
            std::fs::copy(src, &target).map_err(|e| e.to_string())?;
            std::fs::remove_file(src).map_err(|e| e.to_string())?;
            Ok(target)
        }
    }
}

fn same_path(a: &Path, b: &Path) -> bool {
    a.to_string_lossy().eq_ignore_ascii_case(&b.to_string_lossy())
}

fn unique_target(dir: &Path, src: &Path, name: &str) -> PathBuf {
    let mut target = dir.join(name);
    if !target.exists() {
        return target;
    }
    let stem = src
        .file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| name.to_string());
    let ext = src
        .extension()
        .map(|e| format!(".{}", e.to_string_lossy()))
        .unwrap_or_default();
    let mut n = 2;
    loop {
        target = dir.join(format!("{} ({}){}", stem, n, ext));
        if !target.exists() {
            return target;
        }
        n += 1;
        if n > 999 {
            return dir.join(format!("{}-{}{}", stem, crate::now_stamp(), ext));
        }
    }
}
