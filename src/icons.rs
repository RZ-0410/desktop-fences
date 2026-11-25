//! 图标提取与缓存。
//!
//! 走 `IShellItemImageFactory::GetImage` 直接要「指定像素尺寸 + 仅图标」的位图。
//!
//! 为什么不用 `SHGetFileInfo(SHGFI_SYSICONINDEX)` + `SHGetImageList(SHIL_EXTRALARGE)`：
//! 那个索引来自 32px 图像列表，跨列表查表不保证顺序一致，缩放也容易糊。
//! `GetImage` 能按当前 DPI 要精确尺寸，边缘更干净。
//!
//! 注意：图标角上的蓝色双箭头是 **NTFS 压缩标记**（属性 0x800），资源管理器同样显示，
//! 不是渲染缺陷 —— 排查时先看文件属性，别怀疑到自己头上。

use std::collections::HashMap;
use std::ffi::c_void;
use std::mem::{size_of, zeroed};
use std::path::{Path, PathBuf};
use std::ptr::null_mut;

use crate::winapi::*;

const IID_ISHELL_ITEM_IMAGE_FACTORY: GUID = GUID {
    data1: 0xBCC1_8B79,
    data2: 0xBA16,
    data3: 0x442F,
    data4: [0x80, 0xC4, 0x8A, 0x59, 0xC3, 0x0C, 0x46, 0x3B],
};

const SIIGBF_BIGGERSIZEOK: u32 = 0x0001;
const SIIGBF_ICONONLY: u32 = 0x0004;

/// 预乘 alpha 的 BGRA 像素图，尺寸即所需像素尺寸。
pub struct IconImage {
    pub w: i32,
    pub h: i32,
    pub px: Vec<u32>,
}

struct Entry {
    img: Option<IconImage>,
    mtime: u64,
    size: u64,
}

pub struct IconCache {
    map: HashMap<(PathBuf, i32), Entry>,
}

impl IconCache {
    pub fn new() -> Self {
        IconCache { map: HashMap::new() }
    }

    pub fn clear(&mut self) {
        self.map.clear();
    }

    /// 取图标。mtime/size 由调用方从目录扫描结果带过来，
    /// **不再自己 stat**——否则每次重绘都多 N 次文件系统调用。
    pub fn get(&mut self, path: &Path, px_size: i32, mtime: u64, size: u64) -> Option<&IconImage> {
        let key = (path.to_path_buf(), px_size);
        let stale = match self.map.get(&key) {
            Some(e) => e.mtime != mtime || e.size != size,
            None => true,
        };
        if stale {
            let img = unsafe { load_image(path, px_size) };
            self.map.insert(key.clone(), Entry { img, mtime, size });
        }
        self.map.get(&key).and_then(|e| e.img.as_ref())
    }
}

unsafe fn load_image(path: &Path, px: i32) -> Option<IconImage> {
    let p = proc("shell32.dll", "SHCreateItemFromParsingName")?;
    type Create =
        unsafe extern "system" fn(LPCWSTR, *mut c_void, *const GUID, *mut *mut c_void) -> HRESULT;
    let create: Create = std::mem::transmute(p);

    let pw = w(&path.to_string_lossy());
    let mut obj: *mut c_void = null_mut();
    if create(pw.as_ptr(), null_mut(), &IID_ISHELL_ITEM_IMAGE_FACTORY, &mut obj) != S_OK
        || obj.is_null()
    {
        return None;
    }

    let vtbl = *(obj as *mut *const usize);
    type GetImage = unsafe extern "system" fn(*mut c_void, SIZE, u32, *mut HBITMAP) -> HRESULT;
    type Release = unsafe extern "system" fn(*mut c_void) -> u32;
    let get_image: GetImage = std::mem::transmute(*vtbl.add(3));
    let release: Release = std::mem::transmute(*vtbl.add(2));

    let mut hbmp: HBITMAP = null_mut();
    let hr = get_image(
        obj,
        SIZE { cx: px, cy: px },
        SIIGBF_ICONONLY | SIIGBF_BIGGERSIZEOK,
        &mut hbmp,
    );
    release(obj);

    if hr != S_OK || hbmp.is_null() {
        return None;
    }
    let img = bitmap_to_pixels(hbmp);
    DeleteObject(hbmp);
    img
}

/// HBITMAP（32bpp，预乘 ARGB）→ 像素数组。
unsafe fn bitmap_to_pixels(hbmp: HBITMAP) -> Option<IconImage> {
    let mut bm: BITMAP = zeroed();
    if GetObjectW(
        hbmp as HGDIOBJ,
        size_of::<BITMAP>() as i32,
        &mut bm as *mut _ as *mut c_void,
    ) == 0
    {
        return None;
    }
    let w_ = bm.bmWidth;
    let h_ = bm.bmHeight;
    if w_ <= 0 || h_ <= 0 || w_ > 1024 || h_ > 1024 {
        return None;
    }

    let mut bi: BITMAPINFO = zeroed();
    bi.bmiHeader.biSize = size_of::<BITMAPINFOHEADER>() as u32;
    bi.bmiHeader.biWidth = w_;
    bi.bmiHeader.biHeight = -h_; // 自顶向下
    bi.bmiHeader.biPlanes = 1;
    bi.bmiHeader.biBitCount = 32;
    bi.bmiHeader.biCompression = BI_RGB;

    let mut px = vec![0u32; (w_ * h_) as usize];
    let dc = GetDC(null_mut());
    let got = GetDIBits(
        dc,
        hbmp,
        0,
        h_ as u32,
        px.as_mut_ptr() as *mut c_void,
        &mut bi,
        DIB_RGB_COLORS,
    );
    ReleaseDC(null_mut(), dc);
    if got == 0 {
        return None;
    }
    Some(IconImage { w: w_, h: h_, px })
}
