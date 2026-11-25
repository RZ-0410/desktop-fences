//! 分层窗口的绘制画布。
//!
//! 全部走 `UpdateLayeredWindow` + 32bpp 自顶向下 DIB，颜色一律**预乘 alpha**。
//! 为什么要自己写而不是用 `SetLayeredWindowAttributes`：后者只能整窗均匀透明，
//! 圆角只能靠 `SetWindowRgn` 硬切，边缘会有锯齿；要做「圆角 + 半透明面板 + 清晰文字」
//! 必须逐像素控制 alpha。

use std::ffi::c_void;
use std::ptr::null_mut;

use crate::winapi::*;

pub struct Canvas {
    pub w: i32,
    pub h: i32,
    pub hdc: HDC,
    pub hbmp: HBITMAP,
    old: HGDIOBJ,
    pub bits: *mut u32,
}

impl Canvas {
    pub fn new(w: i32, h: i32) -> Option<Canvas> {
        if w <= 0 || h <= 0 || w > 8192 || h > 8192 {
            return None;
        }
        unsafe {
            let hdc = CreateCompatibleDC(null_mut());
            if hdc.is_null() {
                return None;
            }
            let bi = BITMAPINFO {
                bmiHeader: BITMAPINFOHEADER {
                    biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                    biWidth: w,
                    biHeight: -h, // 负数 = 自顶向下，y 向下增大
                    biPlanes: 1,
                    biBitCount: 32,
                    biCompression: BI_RGB,
                    biSizeImage: 0,
                    biXPelsPerMeter: 0,
                    biYPelsPerMeter: 0,
                    biClrUsed: 0,
                    biClrImportant: 0,
                },
                bmiColors: [0],
            };
            let mut bits: *mut c_void = null_mut();
            let hbmp = CreateDIBSection(hdc, &bi, DIB_RGB_COLORS, &mut bits, null_mut(), 0);
            if hbmp.is_null() || bits.is_null() {
                DeleteDC(hdc);
                return None;
            }
            let old = SelectObject(hdc, hbmp);
            let canvas = Canvas { w, h, hdc, hbmp, old, bits: bits as *mut u32 };
            let mut c = canvas;
            c.clear();
            Some(c)
        }
    }

    #[inline]
    fn idx(&self, x: i32, y: i32) -> usize {
        (y as usize) * (self.w as usize) + (x as usize)
    }

    pub fn clear(&mut self) {
        unsafe {
            std::ptr::write_bytes(self.bits, 0, (self.w * self.h) as usize);
        }
    }

    pub fn clear_rect(&mut self, x: i32, y: i32, w: i32, h: i32) {
        let x0 = x.max(0);
        let y0 = y.max(0);
        let x1 = (x + w).min(self.w);
        let y1 = (y + h).min(self.h);
        if x0 >= x1 || y0 >= y1 {
            return;
        }
        unsafe {
            for yy in y0..y1 {
                let p = self.bits.add(self.idx(x0, yy));
                std::ptr::write_bytes(p, 0, (x1 - x0) as usize);
            }
        }
    }

    /// 预乘 alpha 的 source-over 混合。
    #[inline]
    pub fn blend_px(&mut self, x: i32, y: i32, color: [u8; 3], a: f32) {
        if a <= 0.0 || x < 0 || y < 0 || x >= self.w || y >= self.h {
            return;
        }
        let a = a.min(1.0);
        unsafe {
            let p = self.bits.add(self.idx(x, y));
            let d = *p;
            let dr = ((d >> 16) & 0xff) as f32;
            let dg = ((d >> 8) & 0xff) as f32;
            let db = (d & 0xff) as f32;
            let da = ((d >> 24) & 0xff) as f32;
            let inv = 1.0 - a;
            let nr = color[0] as f32 * a + dr * inv;
            let ng = color[1] as f32 * a + dg * inv;
            let nb = color[2] as f32 * a + db * inv;
            let na = 255.0 * a + da * inv;
            *p = ((na as u32 & 0xff) << 24)
                | ((nr as u32 & 0xff) << 16)
                | ((ng as u32 & 0xff) << 8)
                | (nb as u32 & 0xff);
        }
    }

    /// 圆角矩形。`r` 为圆角半径（像素）。覆盖率做 1px 抗锯齿。
    pub fn fill_round_rect(&mut self, x: i32, y: i32, w: i32, h: i32, r: i32, color: [u8; 3], alpha: f32) {
        if w <= 0 || h <= 0 {
            return;
        }
        let r = r.clamp(0, w.min(h) / 2);
        let rf = r as f32;
        for yy in 0..h {
            for xx in 0..w {
                let cx = xx as f32 + 0.5;
                let cy = yy as f32 + 0.5;
                let dx = if cx < rf {
                    rf - cx
                } else if cx > w as f32 - rf {
                    cx - (w as f32 - rf)
                } else {
                    0.0
                };
                let dy = if cy < rf {
                    rf - cy
                } else if cy > h as f32 - rf {
                    cy - (h as f32 - rf)
                } else {
                    0.0
                };
                let cov = if dx <= 0.0 || dy <= 0.0 {
                    1.0
                } else {
                    let d = (dx * dx + dy * dy).sqrt();
                    (rf + 0.5 - d).clamp(0.0, 1.0)
                };
                if cov > 0.0 {
                    self.blend_px(x + xx, y + yy, color, cov * alpha);
                }
            }
        }
    }

    pub fn fill_rect(&mut self, x: i32, y: i32, w: i32, h: i32, color: [u8; 3], alpha: f32) {
        for yy in y..(y + h) {
            for xx in x..(x + w) {
                self.blend_px(xx, yy, color, alpha);
            }
        }
    }

    /// 把一张灰度遮罩（mask 的 R 通道当覆盖率）用 `color` 合成到画布。
    /// 文字与图标都走这条路，好处是预乘 alpha 完全可控。
    pub fn blend_mask(
        &mut self,
        mask: &Canvas,
        src_x: i32,
        src_y: i32,
        dst_x: i32,
        dst_y: i32,
        w: i32,
        h: i32,
        color: [u8; 3],
        alpha: f32,
    ) {
        for yy in 0..h {
            for xx in 0..w {
                let mx = src_x + xx;
                let my = src_y + yy;
                if mx < 0 || my < 0 || mx >= mask.w || my >= mask.h {
                    continue;
                }
                let m = unsafe { *mask.bits.add(mask.idx(mx, my)) };
                let cov = ((m >> 16) & 0xff) as f32 / 255.0;
                if cov > 0.0 {
                    self.blend_px(dst_x + xx, dst_y + yy, color, cov * alpha);
                }
            }
        }
    }

    pub fn draw_icon(&mut self, hicon: HICON, x: i32, y: i32, size: i32) {
        if hicon.is_null() {
            return;
        }
        unsafe {
            DrawIconEx(self.hdc, x, y, hicon, size, size, 0, null_mut(), DI_NORMAL);
        }
    }

    /// 合成一张预乘 alpha 的 BGRA 图标（来自 IShellItemImageFactory）。
    /// 源已经是预乘的，所以直接 `dst = src + dst*(1-a)`，不需要再乘一次。
    pub fn blend_image(&mut self, px: &[u32], iw: i32, ih: i32, x0: i32, y0: i32) {
        if iw <= 0 || ih <= 0 || px.len() < (iw * ih) as usize {
            return;
        }
        for yy in 0..ih {
            for xx in 0..iw {
                let s = px[(yy * iw + xx) as usize];
                let a = (s >> 24) & 0xff;
                if a == 0 {
                    continue;
                }
                let dx = x0 + xx;
                let dy = y0 + yy;
                if dx < 0 || dy < 0 || dx >= self.w || dy >= self.h {
                    continue;
                }
                unsafe {
                    let p = self.bits.add(self.idx(dx, dy));
                    let d = *p;
                    let inv = 255 - a;
                    let sr = (s >> 16) & 0xff;
                    let sg = (s >> 8) & 0xff;
                    let sb = s & 0xff;
                    let dr = (d >> 16) & 0xff;
                    let dg = (d >> 8) & 0xff;
                    let db = d & 0xff;
                    let da = (d >> 24) & 0xff;
                    let nr = (sr + dr * inv / 255).min(255);
                    let ng = (sg + dg * inv / 255).min(255);
                    let nb = (sb + db * inv / 255).min(255);
                    let na = (a + da * inv / 255).min(255);
                    *p = (na << 24) | (nr << 16) | (ng << 8) | nb;
                }
            }
        }
    }

    /// 把画布推给分层窗口。`x, y` 是窗口左上角的屏幕坐标。
    pub fn blit(&self, hwnd: HWND, x: i32, y: i32) {
        unsafe {
            GdiFlush();
            let pt = POINT { x, y };
            let size = SIZE { cx: self.w, cy: self.h };
            let src = POINT { x: 0, y: 0 };
            let blend = BLENDFUNCTION {
                BlendOp: AC_SRC_OVER,
                BlendFlags: 0,
                SourceConstantAlpha: 255,
                AlphaFormat: AC_SRC_ALPHA,
            };
            let screen = GetDC(null_mut());
            UpdateLayeredWindow(hwnd, screen, &pt, &size, self.hdc, &src, 0, &blend, ULW_ALPHA);
            ReleaseDC(null_mut(), screen);
        }
    }
}

impl Drop for Canvas {
    fn drop(&mut self) {
        unsafe {
            SelectObject(self.hdc, self.old);
            DeleteObject(self.hbmp);
            DeleteDC(self.hdc);
        }
    }
}

#[inline]
pub fn rgb(hex: u32) -> [u8; 3] {
    [((hex >> 16) & 0xff) as u8, ((hex >> 8) & 0xff) as u8, (hex & 0xff) as u8]
}

/// 按 DPI 缩放 DIP 值。
#[inline]
pub fn dip(v: i32, dpi: i32) -> i32 {
    ((v as f32) * dpi as f32 / 96.0).round() as i32
}
