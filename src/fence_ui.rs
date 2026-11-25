//! 围栏窗口：一个围栏 = 一个无边框分层窗口。
//!
//! 窗口样式 `WS_POPUP | WS_EX_LAYERED | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE`：
//! - LAYERED：逐像素 alpha
//! - TOOLWINDOW：不出现在任务栏和 Alt+Tab
//! - NOACTIVATE：点击不抢焦点，也不会被顶到普通窗口之上

use std::ffi::c_void;
use std::path::{Path, PathBuf};
use std::ptr::null_mut;
use std::time::UNIX_EPOCH;

use crate::config::{DragMode, FenceCfg};
use crate::gfx::{dip, rgb, Canvas};
use crate::icons::IconCache;
use crate::winapi::*;

pub const CLASS_NAME: &str = "DFFence";
pub const MA_NOACTIVATE: LRESULT = 3;

#[derive(Clone)]
pub struct Item {
    pub name: String,
    pub path: PathBuf,
    pub is_dir: bool,
    /// 按当前 DPI 排好版的两行标签（含换行符）
    pub label: String,
    pub mtime: u64,
    pub size: u64,
    /// 「今天 14:32」/「08-15 09:10」/「2024-08-15」
    pub time_label: String,
}

struct Metrics {
    title: i32,
    search_h: i32,
    pad: i32,
    cell_w: i32,
    cell_h: i32,
    icon: i32,
    /// 文件名两行的高度
    name_h: i32,
    /// 下面那行时间的高度
    time_h: i32,
    radius: i32,
}

impl Metrics {
    fn new(dpi: i32) -> Metrics {
        Metrics {
            title: dip(28, dpi),
            search_h: dip(30, dpi),
            pad: dip(8, dpi),
            cell_w: dip(96, dpi),
            cell_h: dip(104, dpi),
            icon: dip(48, dpi),
            name_h: dip(34, dpi),
            time_h: dip(15, dpi),
            radius: dip(12, dpi),
        }
    }
}

enum Drag {
    None,
    Move { start_cursor: POINT, start_x: i32, start_y: i32 },
    Resize { start_cursor: POINT, start_w: i32, start_h: i32 },
}

pub struct Fence {
    pub hwnd: HWND,
    pub cfg: FenceCfg,
    pub dpi: i32,
    pub items: Vec<Item>,
    pub hover: Option<usize>,
    pub selected: Option<usize>,
    /// 当前滚动的行数（一个围栏装全部文件后必须能滚）
    pub scroll: usize,
    pub icons: IconCache,
    /// 目录内容签名，用来判断要不要重绘
    sig: u64,
    /// 正在有文件被拖到窗口上方
    pub drop_active: bool,
    canvas: Option<Canvas>,
    mask: Option<Canvas>,
    font: HFONT,
    font_title: HFONT,
    font_small: HFONT,
    drag: Drag,
    /// 搜索：开关、关键字、子编辑框、背景刷子、原窗口过程
    pub search_on: bool,
    pub search: String,
    /// 折叠前的展开高度，供恢复使用
    pub h_expanded: i32,
}

impl Fence {
    fn metrics(&self) -> Metrics {
        Metrics::new(self.dpi)
    }

    fn per_page(&self) -> usize {
        let m = self.metrics();
        let rows = ((self.view_h() - self.body_top() - m.pad) / m.cell_h).max(0);
        (rows as usize) * (self.cfg.cols.max(1) as usize)
    }

    /// 标题栏高度（含搜索行）
    fn body_top(&self) -> i32 {
        let m = self.metrics();
        m.title + if self.search_on { m.search_h } else { 0 }
    }

    /// 搜索过滤后的下标
    pub fn visible(&self) -> Vec<usize> {
        if self.search.is_empty() {
            return (0..self.items.len()).collect();
        }
        let q = self.search.to_lowercase();
        self.items
            .iter()
            .enumerate()
            .filter(|(_, it)| it.name.to_lowercase().contains(&q))
            .map(|(i, _)| i)
            .collect()
    }

    /// 临时允许/禁止窗口激活。搜索框要收键盘输入，必须先让围栏能拿焦点；
    /// 否则 WS_EX_NOACTIVATE 会让输入永远落到别的窗口。
    fn set_activate(&mut self, on: bool) {
        unsafe {
            let ex = GetWindowLongW(self.hwnd, GWL_EXSTYLE);
            if on {
                SetWindowLongW(self.hwnd, GWL_EXSTYLE, ex & !(WS_EX_NOACTIVATE as i32));
                SetForegroundWindow(self.hwnd);
            } else {
                SetWindowLongW(self.hwnd, GWL_EXSTYLE, ex | WS_EX_NOACTIVATE as i32);
            }
        }
    }

    pub fn toggle_search(&mut self) {
        self.search_on = !self.search_on;
        crate::dlog(&format!("TOGGLE search_on={}", self.search_on));
        self.set_activate(self.search_on);
        if self.search_on {
            // 自己画搜索框、自己收按键：分层窗口不合成子窗口，EDIT 控件看不见
            unsafe {
                SetFocus(self.hwnd);
            }
        } else {
            self.search.clear();
        }
        self.scroll = 0;
        self.redraw();
    }

    fn clamp_scroll(&mut self) {
        let cols = self.cfg.cols.max(1) as usize;
        let per_page = self.per_page().max(1);
        let max_off = self.visible().len().saturating_sub(per_page);
        let max_scroll = (max_off + cols - 1) / cols;
        if self.scroll > max_scroll {
            self.scroll = max_scroll;
        }
    }

    /// 重新扫描绑定目录。返回 true 表示内容变了、需要重绘。
    pub fn refresh(&mut self) -> bool {
        use crate::config::SortMode;
        let mut items: Vec<Item> = Vec::new();
        for e in scan_dir(&self.cfg.dir) {
            if e.name.is_empty() || e.name.starts_with('.') || e.name == "desktop.ini" || e.hidden {
                continue;
            }
            items.push(Item {
                path: self.cfg.dir.join(&e.name),
                name: e.name,
                is_dir: e.is_dir,
                label: String::new(),
                mtime: e.mtime,
                size: e.size,
                time_label: fmt_time(e.mtime),
            });
        }

        match self.cfg.sort {
            SortMode::Name => items.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase())),
            SortMode::Ext => items.sort_by(|a, b| {
                let ea = a
                    .path
                    .extension()
                    .map(|x| x.to_string_lossy().to_ascii_lowercase())
                    .unwrap_or_default();
                let eb = b
                    .path
                    .extension()
                    .map(|x| x.to_string_lossy().to_ascii_lowercase())
                    .unwrap_or_default();
                ea.cmp(&eb).then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
            }),
            SortMode::Mtime => items.sort_by(|a, b| b.mtime.cmp(&a.mtime)),
            SortMode::Manual => {}
        }

        let mut sig = 0u64;
        for it in &items {
            sig = sig
                .wrapping_mul(1099511628211)
                .wrapping_add(it.mtime)
                .wrapping_add(it.name.len() as u64);
        }
        if sig == self.sig && items.len() == self.items.len() {
            return false;
        }
        self.sig = sig;
        self.items = items;
        self.clamp_scroll();
        self.selected = None;
        self.hover = None;
        true
    }

    /// DPI 变了要重新排版标签
    pub fn relabel(&mut self) {
        for it in &mut self.items {
            it.label.clear();
        }
    }

    fn title_h(&self) -> i32 {
        dip(28, self.dpi)
    }

    fn view_h(&self) -> i32 {
        if self.cfg.collapsed {
            self.title_h()
        } else {
            self.cfg.h
        }
    }

    fn make_fonts(&mut self) {
        unsafe {
            if !self.font.is_null() {
                DeleteObject(self.font);
            }
            if !self.font_title.is_null() {
                DeleteObject(self.font_title);
            }
            if !self.font_small.is_null() {
                DeleteObject(self.font_small);
            }
            let face = w("Microsoft YaHei UI");
            self.font = CreateFontW(
                -dip(12, self.dpi),
                0,
                0,
                0,
                FW_NORMAL,
                0,
                0,
                0,
                DEFAULT_CHARSET,
                0,
                0,
                ANTIALIASED_QUALITY,
                0,
                face.as_ptr(),
            );
            self.font_small = CreateFontW(
                -dip(9, self.dpi),
                0,
                0,
                0,
                FW_NORMAL,
                0,
                0,
                0,
                DEFAULT_CHARSET,
                0,
                0,
                ANTIALIASED_QUALITY,
                0,
                face.as_ptr(),
            );
            self.font_title = CreateFontW(
                -dip(13, self.dpi),
                0,
                0,
                0,
                FW_SEMIBOLD,
                0,
                0,
                0,
                DEFAULT_CHARSET,
                0,
                0,
                ANTIALIASED_QUALITY,
                0,
                face.as_ptr(),
            );
        }
    }

    pub fn apply_geometry(&mut self) {
        let h = self.view_h();
        unsafe {
            SetWindowPos(
                self.hwnd,
                null_mut(),
                self.cfg.x,
                self.cfg.y,
                self.cfg.w,
                h,
                SWP_NOZORDER | SWP_NOACTIVATE,
            );
        }
    }

    pub fn redraw(&mut self) {
        let mut rc = RECT::default();
        unsafe {
            GetWindowRect(self.hwnd, &mut rc);
        }
        let w = rc.w();
        let h = rc.h();
        if w <= 0 || h <= 0 {
            return;
        }

        // 画布尺寸变了就重建（同时也就丢了旧内容，必须整窗重绘）
        let need = self.canvas.as_ref().map_or(true, |c| c.w != w || c.h != h);
        if need {
            self.canvas = Canvas::new(w, h);
            self.mask = None;
        }
        if self.canvas.is_none() {
            crate::dlog(&format!("REDRAW 画布创建失败 {}x{}", w, h));
            return;
        }
        crate::dlog(&format!("REDRAW {}x{} items={} dpi={}", w, h, self.items.len(), self.dpi));

        let m = self.metrics();
        let mask_w = w.max(m.cell_w);
        let mask_h = m.title.max(m.name_h + m.time_h);
        let need_mask = self.mask.as_ref().map_or(true, |c| c.w != mask_w || c.h != mask_h);
        if need_mask {
            self.mask = Canvas::new(mask_w, mask_h);
        }
        if self.mask.is_none() {
            return;
        }

        let panel = rgb(self.cfg.color);
        let border = [
            (panel[0] as u16 + 60).min(255) as u8,
            (panel[1] as u16 + 60).min(255) as u8,
            (panel[2] as u16 + 60).min(255) as u8,
        ];
        let opacity = self.cfg.opacity.clamp(0.15, 1.0);

        // 有文件拖到上方时换 3px 蓝色描边，给一个明确的落点提示
        let (border, border_alpha, inset) = if self.drop_active {
            ([91u8, 157, 255], 0.95, 3)
        } else {
            (border, 0.45, 1)
        };

        {
            let canvas = self.canvas.as_mut().unwrap();
            canvas.clear();
            canvas.fill_round_rect(0, 0, w, h, m.radius, border, border_alpha);
            canvas.fill_round_rect(
                inset,
                inset,
                w - inset * 2,
                h - inset * 2,
                (m.radius - inset).max(0),
                panel,
                opacity,
            );
        }

        // 标题
        let title_color = [232u8, 236, 245];
        let sub_color = [150u8, 158, 176];
        {
            let mask = self.mask.take().unwrap();
            let canvas = self.canvas.as_mut().unwrap();
            let mut mask = mask;
            let name_w = (w - m.pad * 2 - dip(56, self.dpi)).max(20);
            // 文字框给满整个标题栏高度：字号只有 24px，但 DT_VCENTER 的墨水
            // 可能略超字号高度，框太小会被遮罩切掉顶部（实测踩过）。
            draw_text(
                canvas,
                &mut mask,
                &self.cfg.name,
                m.pad,
                0,
                name_w,
                m.title,
                self.font_title,
                title_color,
                1.0,
                DT_LEFT | DT_SINGLELINE | DT_VCENTER | DT_END_ELLIPSIS,
            );

            // 右侧：搜索按钮（最右）+ 折叠箭头 + 锁标记 + 数量
            let mut rx = w - m.pad;
            let sx = rx - dip(48, self.dpi);
            draw_text(
                canvas,
                &mut mask,
                if self.search_on { "✕" } else { crate::i18n::t("search") },
                sx,
                0,
                dip(48, self.dpi),
                m.title,
                self.font,
                if self.search_on { [232u8, 236, 245] } else { sub_color },
                0.95,
                DT_CENTER | DT_SINGLELINE | DT_VCENTER,
            );
            rx = sx;
            let chev = if self.cfg.collapsed { "▸" } else { "▾" };
            rx -= dip(14, self.dpi);
            draw_text(
                canvas,
                &mut mask,
                chev,
                rx,
                0,
                dip(14, self.dpi),
                m.title,
                self.font,
                sub_color,
                0.9,
                DT_CENTER | DT_SINGLELINE | DT_VCENTER,
            );
            if self.cfg.locked {
                rx -= dip(18, self.dpi);
                draw_lock(canvas, rx, (m.title - dip(12, self.dpi)) / 2, dip(11, self.dpi), sub_color);
            }
            let count = format!("{}", self.items.len());
            rx -= dip(22, self.dpi);
            draw_text(
                canvas,
                &mut mask,
                &count,
                rx,
                0,
                dip(22, self.dpi),
                m.title,
                self.font,
                sub_color,
                0.85,
                DT_RIGHT | DT_SINGLELINE | DT_VCENTER,
            );

            // 标题分隔线
            if !self.cfg.collapsed {
                canvas.fill_rect(m.pad, m.title - 1, w - m.pad * 2, 1, [255, 255, 255], 0.07);
            }
            self.mask = Some(mask);
        }

        // 内容区
        if !self.cfg.collapsed {
            let top = self.body_top();
            let rows = ((h - top - m.pad) / m.cell_h).max(0);
            let cols = self.cfg.cols.max(1);
            let visible = (rows * cols) as usize;
            let vis = self.visible();

            let mask = self.mask.take().unwrap();
            let canvas = self.canvas.as_mut().unwrap();
            let mut mask = mask;

            // 搜索行：底色 + 已输入的关键字
            if self.search_on {
                let hh = m.search_h - dip(8, self.dpi);
                let bx = m.pad;
                let by = m.title + dip(4, self.dpi);
                canvas.fill_round_rect(bx, by, w - m.pad * 2, hh, dip(6, self.dpi), [20u8, 22, 28], 0.72);
                let shown = if self.search.is_empty() {
                    crate::i18n::t("search_hint").to_string()
                } else {
                    format!("{}|", self.search)
                };
                let (col, al) = if self.search.is_empty() {
                    ([120u8, 128, 144], 0.8)
                } else {
                    ([232u8, 236, 245], 1.0)
                };
                draw_text(
                    canvas,
                    &mut mask,
                    &shown,
                    bx + dip(8, self.dpi),
                    by,
                    w - m.pad * 2 - dip(16, self.dpi),
                    hh,
                    self.font,
                    col,
                    al,
                    DT_LEFT | DT_SINGLELINE | DT_VCENTER,
                );
            }

            if self.items.is_empty() {
                let cy = top + (h - top) / 2 - dip(18, self.dpi);
                draw_text(
                    canvas,
                    &mut mask,
                    crate::i18n::t("empty_title"),
                    0,
                    cy,
                    w,
                    dip(22, self.dpi),
                    self.font_title,
                    [170u8, 177, 192],
                    0.75,
                    DT_CENTER | DT_SINGLELINE | DT_VCENTER,
                );
                draw_text(
                    canvas,
                    &mut mask,
                    crate::i18n::t("empty_hint"),
                    0,
                    cy + dip(24, self.dpi),
                    w,
                    dip(20, self.dpi),
                    self.font,
                    [130u8, 138, 154],
                    0.7,
                    DT_CENTER | DT_SINGLELINE,
                );
            } else if vis.is_empty() {
                let cy = top + (h - top) / 2 - dip(18, self.dpi);
                draw_text(
                    canvas,
                    &mut mask,
                    crate::i18n::t("no_match"),
                    0,
                    cy,
                    w,
                    dip(22, self.dpi),
                    self.font_title,
                    [170u8, 177, 192],
                    0.75,
                    DT_CENTER | DT_SINGLELINE | DT_VCENTER,
                );
            } else {
                let start = self.scroll * cols as usize;
                for vi in 0..visible {
                    let pos = start + vi;
                    if pos >= vis.len() {
                        break;
                    }
                    let idx = vis[pos];
                    let col = (vi as i32) % cols;
                    let row = (vi as i32) / cols;
                    let cx = m.pad + col * m.cell_w;
                    let cy = top + m.pad + row * m.cell_h;
                    let hovered = self.hover == Some(idx);
                    let selected = self.selected == Some(idx);
                    if hovered || selected {
                        let a = if selected { 0.16 } else { 0.09 };
                        canvas.fill_round_rect(
                            cx + 2,
                            cy + 2,
                            m.cell_w - 4,
                            m.cell_h - 4,
                            dip(8, self.dpi),
                            [255, 255, 255],
                            a,
                        );
                    }
                    let (ipath, isize_, imtime, ifsize) = {
                        let it = &self.items[idx];
                        (it.path.clone(), m.icon, it.mtime, it.size)
                    };
                    let icon = self.icons.get(&ipath, isize_, imtime, ifsize);
                    let font = self.font;
                    let text_left = cx + 2;
                    let text_top = cy + m.icon + dip(2, self.dpi);
                    let text_w = m.cell_w - 4;
                    let font_small = self.font_small;
                    let item = &mut self.items[idx];
                    if item.label.is_empty() {
                        item.label =
                            layout_label(mask.hdc, font, &item.name, m.cell_w - dip(6, self.dpi));
                    }
                    if let Some(img) = icon {
                        let ix = cx + (m.cell_w - img.w) / 2;
                        let iy = cy + (m.icon - img.h) / 2;
                        canvas.blend_image(&img.px, img.w, img.h, ix, iy);
                    }
                    draw_text(
                        canvas,
                        &mut mask,
                        &item.label,
                        text_left,
                        text_top,
                        text_w,
                        m.name_h,
                        font,
                        [216u8, 220, 232],
                        0.95,
                        DT_CENTER | DT_WORDBREAK | DT_NOPREFIX,
                    );
                    if !item.time_label.is_empty() {
                        draw_text(
                            canvas,
                            &mut mask,
                            &item.time_label,
                            text_left,
                            text_top + m.name_h,
                            text_w,
                            m.time_h,
                            font_small,
                            [150u8, 158, 176],
                            0.85,
                            DT_CENTER | DT_SINGLELINE | DT_NOPREFIX,
                        );
                    }
                }

                // 多于一屏时右下角给个页码，避免用户以为文件丢了。
                // 注意 scroll 的单位是「行」，页码要按每页行数换算，别直接拿它当页号。
                if vis.len() > visible {
                    let rows_pp = (visible / cols as usize).max(1);
                    let page = self.scroll / rows_pp + 1;
                    let total = (vis.len() + visible - 1) / visible;
                    let hint = format!("{}/{}", page.min(total.max(1)), total.max(1));
                    draw_text(
                        canvas,
                        &mut mask,
                        &hint,
                        w - dip(56, self.dpi),
                        h - dip(20, self.dpi),
                        dip(48, self.dpi),
                        dip(16, self.dpi),
                        self.font,
                        [150u8, 158, 176],
                        0.85,
                        DT_RIGHT | DT_SINGLELINE,
                    );
                }
            }
            self.mask = Some(mask);
        }

        if let Some(canvas) = self.canvas.as_ref() {
            canvas.blit(self.hwnd, rc.left, rc.top);
            crate::dlog(&format!("REDRAW 完成 blit ({},{})", rc.left, rc.top));
        }
    }

    pub fn set_collapsed(&mut self, collapsed: bool) {
        if self.cfg.collapsed == collapsed {
            return;
        }
        if collapsed {
            self.h_expanded = self.cfg.h;
            self.cfg.collapsed = true;
        } else {
            self.cfg.collapsed = false;
            self.cfg.h = self.h_expanded.max(self.title_h() * 2);
        }
        self.apply_geometry();
        self.redraw();
    }

    fn hit(&self, x: i32, y: i32) -> Hit {
        let m = self.metrics();
        let w = self.cfg.w;
        let h = self.view_h();
        if !self.cfg.locked && !self.cfg.collapsed {
            if x >= w - dip(16, self.dpi) && y >= h - dip(16, self.dpi) {
                return Hit::Resize;
            }
        }
        if y < m.title {
            let sx = w - m.pad - dip(48, self.dpi);
            if x >= sx {
                return Hit::Search;
            }
            return Hit::Title;
        }
        if self.cfg.collapsed {
            return Hit::None;
        }
        let vis = self.visible();
        if vis.is_empty() {
            return Hit::None;
        }
        let ly = y - self.body_top() - m.pad;
        if ly < 0 || x < m.pad {
            return Hit::None;
        }
        let col = (x - m.pad) / m.cell_w;
        let row = ly / m.cell_h;
        if col >= self.cfg.cols.max(1) {
            return Hit::None;
        }
        let cols = self.cfg.cols.max(1) as usize;
        let pos = self.scroll * cols + (row * self.cfg.cols + col) as usize;
        if pos < vis.len() {
            Hit::Cell(vis[pos])
        } else {
            Hit::None
        }
    }
}

enum Hit {
    None,
    Search,
    Title,
    Resize,
    Cell(usize),
}

// ------------------------------------------------------------------ 文字

fn draw_text(
    canvas: &mut Canvas,
    mask: &mut Canvas,
    text: &str,
    x: i32,
    y: i32,
    width: i32,
    height: i32,
    font: HFONT,
    color: [u8; 3],
    alpha: f32,
    flags: UINT,
) {
    if width <= 0 || height <= 0 || text.is_empty() {
        return;
    }
    mask.clear_rect(0, 0, width, height);
    unsafe {
        let old = SelectObject(mask.hdc, font);
        SetBkMode(mask.hdc, TRANSPARENT);
        SetTextColor(mask.hdc, 0x00FF_FFFF);
        let tw = w(text);
        let mut rc = RECT::new(0, 0, width, height);
        DrawTextW(mask.hdc, tw.as_ptr(), -1, &mut rc, flags);
        SelectObject(mask.hdc, old);
    }
    canvas.blend_mask(mask, 0, 0, x, y, width, height, color, alpha);
}

/// 目录条目：全部元数据来自同一次 FindFirstFileW。
pub struct Scanned {
    pub name: String,
    pub is_dir: bool,
    pub hidden: bool,
    pub mtime: u64,
    pub size: u64,
}

/// 一次系统调用拿完整目录列表。
///
/// 对比 std::fs::read_dir + metadata：后者是 1 + N 次内核往返，178 个文件就是 179 次；
/// 这里只有 1 次。空闲时每 3 秒的刷新因此从「200 次调用」降到「1 次」。
fn scan_dir(dir: &Path) -> Vec<Scanned> {
    let mut out = Vec::new();
    let pattern = w(&format!("{}\\*", dir.display()));
    unsafe {
        let mut data: WIN32_FIND_DATAW = std::mem::zeroed();
        let h = FindFirstFileW(pattern.as_ptr(), &mut data);
        if h == INVALID_HANDLE_VALUE {
            return out;
        }
        loop {
            let nlen = data.cFileName.iter().position(|&c| c == 0).unwrap_or(260);
            let name = String::from_utf16_lossy(&data.cFileName[..nlen]);
            let attr = data.dwFileAttributes;
            if !name.is_empty() && name != "." && name != ".." {
                let ticks = ((data.ftLastWriteTime.dwHighDateTime as u64) << 32)
                    | data.ftLastWriteTime.dwLowDateTime as u64;
                let mtime = (ticks / 10_000_000).saturating_sub(11_644_473_600);
                out.push(Scanned {
                    name,
                    is_dir: attr & FILE_ATTRIBUTE_DIRECTORY != 0,
                    hidden: attr & (FILE_ATTRIBUTE_HIDDEN | FILE_ATTRIBUTE_SYSTEM) != 0,
                    mtime,
                    size: ((data.nFileSizeHigh as u64) << 32) | data.nFileSizeLow as u64,
                });
            }
            if FindNextFileW(h, &mut data) == 0 {
                break;
            }
        }
        FindClose(h);
    }
    out
}

/// 把 unix 秒格式化成本地时间：今天只显示时分，今年显示月日，跨年显示年月日。
fn fmt_time(unix: u64) -> String {
    if unix == 0 {
        return String::new();
    }
    let ticks = (unix + 11_644_473_600) * 10_000_000;
    let ft = FILETIME {
        dwLowDateTime: (ticks & 0xffff_ffff) as u32,
        dwHighDateTime: (ticks >> 32) as u32,
    };
    unsafe {
        let mut local: FILETIME = std::mem::zeroed();
        if FileTimeToLocalFileTime(&ft, &mut local) == 0 {
            return String::new();
        }
        let mut st: SYSTEMTIME = std::mem::zeroed();
        if FileTimeToSystemTime(&local, &mut st) == 0 {
            return String::new();
        }
        let mut now: SYSTEMTIME = std::mem::zeroed();
        GetLocalTime(&mut now);
        if st.wYear == now.wYear && st.wMonth == now.wMonth && st.wDay == now.wDay {
            format!("今天 {:02}:{:02}", st.wHour, st.wMinute)
        } else if st.wYear == now.wYear {
            format!("{:02}-{:02} {:02}:{:02}", st.wMonth, st.wDay, st.wHour, st.wMinute)
        } else {
            format!("{}-{:02}-{:02}", st.wYear, st.wMonth, st.wDay)
        }
    }
}

unsafe fn text_width(hdc: HDC, s: &str) -> i32 {
    let mut buf: Vec<u16> = s.encode_utf16().collect();
    if buf.is_empty() {
        return 0;
    }
    let mut sz = SIZE::default();
    GetTextExtentPoint32W(hdc, buf.as_mut_ptr(), buf.len() as i32, &mut sz);
    sz.cx
}

/// 按实际字宽排成最多两行，排不下就用省略号收尾。
/// 不能用 `DT_END_ELLIPSIS`——它在多行 + 居中下不生效。
fn wrap_two_lines(hdc: HDC, font: HFONT, text: &str, max_w: i32) -> (String, bool) {
    if text.is_empty() || max_w <= 0 {
        return (String::new(), false);
    }
    unsafe {
        let old = SelectObject(hdc, font);
        let chars: Vec<char> = text.chars().collect();
        let mut lines: Vec<String> = Vec::new();
        let mut cur = String::new();
        let mut cur_w = 0i32;
        let mut i = 0usize;

        while i < chars.len() {
            let mut buf = [0u16; 4];
            let enc = chars[i].encode_utf16(&mut buf);
            let mut sz = SIZE::default();
            GetTextExtentPoint32W(hdc, enc.as_ptr(), enc.len() as i32, &mut sz);
            if !cur.is_empty() && cur_w + sz.cx > max_w {
                lines.push(cur.clone());
                cur.clear();
                cur_w = 0;
                if lines.len() == 2 {
                    break;
                }
                continue;
            }
            cur.push(chars[i]);
            cur_w += sz.cx;
            i += 1;
        }
        if lines.len() < 2 && !cur.is_empty() {
            lines.push(cur);
        }
        let truncated = i < chars.len();
        if truncated {
            if let Some(last) = lines.last_mut() {
                while !last.is_empty() {
                    let mut probe = last.clone();
                    probe.push('…');
                    if text_width(hdc, &probe) <= max_w {
                        break;
                    }
                    last.pop();
                }
                last.push('…');
            }
        }
        SelectObject(hdc, old);
        (lines.join("\n"), truncated)
    }
}

fn measure_with(hdc: HDC, font: HFONT, s: &str) -> i32 {
    unsafe {
        let old = SelectObject(hdc, font);
        let width = text_width(hdc, s);
        SelectObject(hdc, old);
        width
    }
}

/// 文件名排版：优先保住主名，别把扩展名劈成两半。
/// 触发条件有两种：超过两行，或者第二行只剩一两个字符（比如只剩 "cx"）。
fn layout_label(hdc: HDC, font: HFONT, name: &str, max_w: i32) -> String {
    let (full, cut) = wrap_two_lines(hdc, font, name, max_w);
    let stem_split = name.rsplit_once('.');

    let need_stem = if cut {
        true
    } else {
        // 末行太短（不到三分之一宽）说明扩展名被切开了
        match full.rsplit('\n').next() {
            Some(last) if !last.is_empty() => measure_with(hdc, font, last) * 3 < max_w,
            _ => false,
        }
    };

    if need_stem {
        if let Some((stem, ext)) = stem_split {
            if !stem.is_empty() && ext.len() <= 6 {
                let (s, _) = wrap_two_lines(hdc, font, stem, max_w);
                return s;
            }
        }
    }
    full
}

fn draw_lock(canvas: &mut Canvas, x: i32, y: i32, size: i32, color: [u8; 3]) {
    let body_w = size;
    let body_h = (size as f32 * 0.62) as i32;
    let body_y = y + size - body_h;
    canvas.fill_rect(x, body_y, body_w, body_h, color, 0.75);
    // 锁梁：一个扁圆环的近似
    let r = size as f32 * 0.30;
    let cx = x as f32 + size as f32 / 2.0;
    let cy = body_y as f32;
    let mut yy = -(r as i32);
    while yy <= 0 {
        let mut xx = -(r as i32) - 1;
        while xx <= r as i32 + 1 {
            let d = ((xx as f32) * (xx as f32) + (yy as f32) * (yy as f32)).sqrt();
            if (d - r).abs() < 1.0 && yy <= 0 {
                canvas.blend_px(cx as i32 + xx, cy as i32 + yy, color, 0.75);
            }
            xx += 1;
        }
        yy += 1;
    }
}

// ------------------------------------------------------------------ 窗口类

pub unsafe fn register(hinst: HINSTANCE) -> bool {
    SetLastError(0);
    let cls = w(CLASS_NAME);
    let cursor = LoadCursorW(null_mut(), IDC_ARROW);
    let wc = WNDCLASSEXW {
        cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
        style: 0x0008 | 0x0002, // CS_DBLCLKS | CS_HREDRAW 位（VA 可选）
        lpfnWndProc: Some(proc),
        cbClsExtra: 0,
        cbWndExtra: 0,
        hInstance: hinst,
        hIcon: null_mut(),
        hCursor: cursor,
        hbrBackground: null_mut(),
        lpszMenuName: null_mut(),
        lpszClassName: cls.as_ptr(),
        hIconSm: null_mut(),
    };
    let ok = RegisterClassExW(&wc);
    crate::dlog(&format!("REGISTER DFFence -> {} err={}", ok, GetLastError()));
    ok != 0
}

pub unsafe fn create(hinst: HINSTANCE, cfg: FenceCfg, dpi: i32) -> Option<Fence> {
    let cls = w(CLASS_NAME);
    let h = if cfg.collapsed { dip(28, dpi) } else { cfg.h };
    SetLastError(0);
    let hwnd = CreateWindowExW(
        WS_EX_LAYERED | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE,
        cls.as_ptr(),
        null_mut(),
        WS_POPUP,
        cfg.x,
        cfg.y,
        cfg.w,
        h,
        null_mut(),
        null_mut(),
        hinst,
        null_mut(),
    );
    if hwnd.is_null() {
        crate::dlog(&format!(
            "CREATE fence 失败 err={} cls={} style=0x{:08X} rect=({},{},{}x{})",
            GetLastError(),
            CLASS_NAME,
            WS_EX_LAYERED | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE,
            cfg.x,
            cfg.y,
            cfg.w,
            h
        ));
        return None;
    }
    crate::dlog("CREATE fence 窗口 OK");
    let mut f = Fence {
        hwnd,
        h_expanded: cfg.h_expanded.max(cfg.h),
        cfg,
        dpi,
        items: Vec::new(),
        hover: None,
        selected: None,
        scroll: 0,
        icons: IconCache::new(),
        sig: 0,
        drop_active: false,
        canvas: None,
        mask: None,
        font: null_mut(),
        font_title: null_mut(),
        font_small: null_mut(),
        drag: Drag::None,
        search_on: false,
        search: String::new(),
    };

    crate::dlog("CREATE init 字段 OK");
    f.make_fonts();
    crate::dlog("CREATE 字体 OK");
    f.refresh();
    crate::dlog(&format!("CREATE 扫描 OK items={}", f.items.len()));
    f.redraw();
    crate::dlog("CREATE 首绘 OK");
    // 拖入模式：默认走已实测的 WM_DROPFILES，OLE 需要配置 drag_mode=ole 才启用
    match crate::app().cfg.global.drag_mode {
        DragMode::Ole => {
            if !crate::drop::register(hwnd) {
                DragAcceptFiles(hwnd, 1);
            }
        }
        DragMode::DropFiles => {
            DragAcceptFiles(hwnd, 1);
        }
    }
    ShowWindow(hwnd, SW_SHOWNOACTIVATE);
    f.redraw();
    Some(f)
}

// ------------------------------------------------------------------ 窗口过程

pub unsafe extern "system" fn proc(hwnd: HWND, msg: UINT, wp: WPARAM, lp: LPARAM) -> LRESULT {
    match msg {
        WM_NCCREATE => {
            crate::dlog("PROC WM_NCCREATE");
            return DefWindowProcW(hwnd, msg, wp, lp);
        }
        WM_CREATE => {
            crate::dlog("PROC WM_CREATE");
            return DefWindowProcW(hwnd, msg, wp, lp);
        }
        WM_CHAR => {
            crate::dlog(&format!("WM_CHAR wp={}", wp));
            if let Some(f) = crate::fence_mut(hwnd) {
                if f.search_on {
                    let ch = wp as u32;
                    if ch == 8 {
                        f.search.pop();
                    } else if ch >= 32 {
                        if let Some(c) = char::from_u32(ch) {
                            f.search.push(c);
                        }
                    }
                    f.scroll = 0;
                    f.redraw();
                }
            }
            return 0;
        }
        WM_KEYDOWN => {
            crate::dlog(&format!("WM_KEYDOWN wp={}", wp));
            if wp == VK_ESCAPE {
                if let Some(f) = crate::fence_mut(hwnd) {
                    if f.search_on {
                        f.search_on = false;
                        f.search.clear();
                        f.set_activate(false);
                        f.scroll = 0;
                        f.redraw();
                        return 0;
                    }
                }
            }
            return DefWindowProcW(hwnd, msg, wp, lp);
        }
        WM_NCDESTROY => {
            RevokeDragDrop(hwnd);
            return DefWindowProcW(hwnd, msg, wp, lp);
        }
        WM_MOUSEACTIVATE => return MA_NOACTIVATE,
        WM_ERASEBKGND => return 1,
        WM_PAINT => return 0,

        WM_SETCURSOR => {
            if let Some(f) = crate::fence_mut(hwnd) {
                let mut pt = POINT::default();
                GetCursorPos(&mut pt);
                let mut rc = RECT::default();
                GetWindowRect(hwnd, &mut rc);
                let lx = pt.x - rc.left;
                let ly = pt.y - rc.top;
                let m = f.metrics();
                let w = f.cfg.w;
                let h = f.view_h();
                let on_grip = !f.cfg.locked
                    && !f.cfg.collapsed
                    && lx >= w - dip(16, f.dpi)
                    && ly >= h - dip(16, f.dpi);
                let _ = m;
                if on_grip {
                    SetCursor(LoadCursorW(null_mut(), IDC_SIZENWSE));
                    return 1;
                }
            }
            return DefWindowProcW(hwnd, msg, wp, lp);
        }

        WM_LBUTTONDOWN => {
            let x = (lp & 0xffff) as i16 as i32;
            let y = ((lp >> 16) & 0xffff) as i16 as i32;
            crate::dlog(&format!("DOWN-IN hwnd={:?} local=({},{})", hwnd, x, y));
            if let Some(f) = crate::fence_mut(hwnd) {
                let hit = f.hit(x, y);
                crate::dlog(&format!("DOWN local=({},{}) hit={} collapsed={} h={} he={}",
                    x, y,
                    match hit { Hit::Title=>"title", Hit::Resize=>"resize", Hit::Search=>"search", Hit::Cell(_)=>"cell", Hit::None=>"none" },
                    f.cfg.collapsed, f.cfg.h, f.h_expanded));
                match hit {
                    Hit::Title => {
                        if f.cfg.locked {
                            return 0;
                        }
                        let mut pt = POINT::default();
                        GetCursorPos(&mut pt);
                        f.drag = Drag::Move {
                            start_cursor: pt,
                            start_x: f.cfg.x,
                            start_y: f.cfg.y,
                        };
                        SetCapture(hwnd);
                    }
                    Hit::Resize => {
                        let mut pt = POINT::default();
                        GetCursorPos(&mut pt);
                        f.drag = Drag::Resize {
                            start_cursor: pt,
                            start_w: f.cfg.w,
                            start_h: f.cfg.h,
                        };
                        SetCapture(hwnd);
                    }
                    Hit::Search => {
                        f.toggle_search();
                    }
                    Hit::Cell(i) => {
                        f.selected = Some(i);
                        f.redraw();
                    }
                    Hit::None => {
                        f.selected = None;
                        f.redraw();
                    }
                }
            }
            return 0;
        }

        WM_MOUSEMOVE => {
            let x = (lp & 0xffff) as i16 as i32;
            let y = ((lp >> 16) & 0xffff) as i16 as i32;
            if let Some(f) = crate::fence_mut(hwnd) {
                let mut pt = POINT::default();
                GetCursorPos(&mut pt);

                match f.drag {
                    Drag::Move { start_cursor, start_x, start_y } => {
                        let mut nx = start_x + (pt.x - start_cursor.x);
                        let mut ny = start_y + (pt.y - start_cursor.y);
                        // 自由拖动：不做网点量化，只在开启吸附且没按 Alt 时贴屏幕四边
                        let alt = GetKeyState(VK_MENU) < 0;
                        if f.cfg.snap && !alt {
                            let (mx, my, mw, mh) = virtual_screen();
                            let edge = dip(12, f.dpi);
                            if (nx - mx).abs() < edge {
                                nx = mx;
                            }
                            if ((mx + mw) - (nx + f.cfg.w)).abs() < edge {
                                nx = mx + mw - f.cfg.w;
                            }
                            if (ny - my).abs() < edge {
                                ny = my;
                            }
                            if (my + mh - (ny + f.view_h())).abs() < edge {
                                ny = my + mh - f.view_h();
                            }
                        }
                        f.cfg.x = nx;
                        f.cfg.y = ny;
                        SetWindowPos(hwnd, null_mut(), nx, ny, 0, 0, SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE);
                    }
                    Drag::Resize { start_cursor, start_w, start_h } => {
                        let nw = (start_w + (pt.x - start_cursor.x)).max(dip(160, f.dpi));
                        let nh = (start_h + (pt.y - start_cursor.y)).max(dip(120, f.dpi));
                        f.cfg.w = nw;
                        f.cfg.h = nh;
                        f.h_expanded = nh;
                        SetWindowPos(hwnd, null_mut(), 0, 0, nw, nh, SWP_NOMOVE | SWP_NOZORDER | SWP_NOACTIVATE);
                        f.redraw();
                    }
                    Drag::None => {
                        let h = f.hit(x, y);
                        let new_hover = match h {
                            Hit::Cell(i) => Some(i),
                            _ => None,
                        };
                        if new_hover != f.hover {
                            f.hover = new_hover;
                            f.redraw();
                        }
                        let mut tme = TRACKMOUSEEVENT {
                            cbSize: std::mem::size_of::<TRACKMOUSEEVENT>() as u32,
                            dwFlags: TME_LEAVE,
                            hwndTrack: hwnd,
                            dwHoverTime: 0,
                        };
                        TrackMouseEvent(&mut tme);
                    }
                }
            }
            return 0;
        }

        WM_LBUTTONUP => {
            if let Some(f) = crate::fence_mut(hwnd) {
                let was_drag = !matches!(f.drag, Drag::None);
                f.drag = Drag::None;
                if was_drag {
                    ReleaseCapture();
                    crate::save_config();
                }
            }
            return 0;
        }

        WM_LBUTTONDBLCLK => {
            let x = (lp & 0xffff) as i16 as i32;
            let y = ((lp >> 16) & 0xffff) as i16 as i32;
            crate::dlog(&format!("DBLCLK local=({},{})", x, y));
            if let Some(f) = crate::fence_mut(hwnd) {
                match f.hit(x, y) {
                    Hit::Title => {
                        if !f.cfg.locked {
                            let collapsed = f.cfg.collapsed;
                            f.set_collapsed(!collapsed);
                            crate::save_config();
                        }
                    }
                    Hit::Cell(i) => {
                        if let Some(item) = f.items.get(i) {
                            crate::shell::open_path(&item.path);
                        }
                    }
                    _ => {}
                }
            }
            return 0;
        }

        WM_MOUSELEAVE => {
            if let Some(f) = crate::fence_mut(hwnd) {
                if f.hover.is_some() {
                    f.hover = None;
                    f.redraw();
                }
            }
            return 0;
        }

        WM_CAPTURECHANGED => {
            if let Some(f) = crate::fence_mut(hwnd) {
                f.drag = Drag::None;
            }
            return 0;
        }

        WM_DROPFILES => {
            crate::drop::handle_dropfiles(hwnd, wp as HANDLE);
            return 0;
        }

        WM_MOUSEWHEEL => {
            let delta = ((wp >> 16) & 0xffff) as i16 as i32;
            if let Some(f) = crate::fence_mut(hwnd) {
                if !f.cfg.collapsed {
                    if delta > 0 {
                        f.scroll = f.scroll.saturating_sub(1);
                    } else {
                        f.scroll += 1;
                    }
                    f.clamp_scroll();
                    f.selected = None;
                    f.hover = None;
                    f.redraw();
                }
            }
            return 0;
        }

        WM_RBUTTONUP => {
            let mut pt = POINT::default();
            GetCursorPos(&mut pt);
            let mut wr = RECT::default();
            GetWindowRect(hwnd, &mut wr);
            let (lx, ly) = (pt.x - wr.left, pt.y - wr.top);
            let cell = crate::fence_mut(hwnd).and_then(|f| match f.hit(lx, ly) {
                Hit::Cell(i) => Some(i),
                _ => None,
            });
            match cell {
                Some(i) => crate::show_item_menu(hwnd, i, pt),
                None => crate::show_fence_menu(hwnd, pt),
            }
            return 0;
        }

        WM_DPICHANGED => {
            let dpi = (wp & 0xffff) as i32;
            if let Some(f) = crate::fence_mut(hwnd) {
                f.dpi = dpi.max(72);
                f.make_fonts();
                f.relabel();
                f.canvas = None;
                f.mask = None;
                let r = &*(lp as *const RECT);
                f.cfg.x = r.left;
                f.cfg.y = r.top;
                f.cfg.w = r.w();
                if !f.cfg.collapsed {
                    f.cfg.h = r.h();
                    f.h_expanded = r.h();
                }
                SetWindowPos(hwnd, null_mut(), r.left, r.top, r.w(), f.view_h(), SWP_NOZORDER | SWP_NOACTIVATE);
                f.redraw();
            }
            return 0;
        }

        WM_DISPLAYCHANGE => {
            if let Some(f) = crate::fence_mut(hwnd) {
                let (mx, my, mw, mh) = virtual_screen();
                let w = f.cfg.w.min(mw);
                let h = f.view_h().min(mh);
                let x = f.cfg.x.clamp(mx, mx + mw - w);
                let y = f.cfg.y.clamp(my, my + mh - h);
                if x != f.cfg.x || y != f.cfg.y {
                    f.cfg.x = x;
                    f.cfg.y = y;
                    f.apply_geometry();
                }
                f.redraw();
            }
            return 0;
        }

        WM_SETTINGCHANGE => {
            if let Some(f) = crate::fence_mut(hwnd) {
                f.canvas = None;
                f.redraw();
            }
            return 0;
        }

        _ => {}
    }
    DefWindowProcW(hwnd, msg, wp, lp)
}

pub fn dip_pub(v: i32, dpi: i32) -> i32 {
    dip(v, dpi)
}

fn virtual_screen() -> (i32, i32, i32, i32) {
    unsafe {
        (
            GetSystemMetrics(SM_XVIRTUALSCREEN),
            GetSystemMetrics(SM_YVIRTUALSCREEN),
            GetSystemMetrics(SM_CXVIRTUALSCREEN),
            GetSystemMetrics(SM_CYVIRTUALSCREEN),
        )
    }
}

/// 让围栏窗口停在桌面之上、普通应用窗口之下。
///
/// 注意：这里**必须**用 `HWND_BOTTOM`。用 `SetWindowPos(hwnd, progman, …)` 会把窗口
/// 插到 Progman **之后**（也就是图标层下面），后续点击会被桌面吃掉 —— 实测踩过。
pub unsafe fn reorder(hwnd: HWND) {
    SetWindowPos(
        hwnd,
        HWND_BOTTOM,
        0,
        0,
        0,
        0,
        SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE,
    );
}

pub unsafe fn set_visible(hwnd: HWND, visible: bool) {
    ShowWindow(hwnd, if visible { SW_SHOWNOACTIVATE } else { SW_HIDE });
}

/// 读窗口的屏幕矩形。
pub unsafe fn rect_of(hwnd: HWND) -> RECT {
    let mut rc = RECT::default();
    GetWindowRect(hwnd, &mut rc);
    rc
}

pub unsafe fn default_dpi() -> i32 {
    let hdc = GetDC(null_mut());
    let dpi = GetDeviceCaps(hdc, LOGPIXELSX);
    ReleaseDC(null_mut(), hdc);
    if dpi <= 0 {
        96
    } else {
        dpi
    }
}

pub unsafe fn get_dpi_for_window(hwnd: HWND) -> i32 {
    type Fn = unsafe extern "system" fn(HWND) -> u32;
    if let Some(p) = crate::winapi::proc("user32.dll", "GetDpiForWindow") {
        let f: Fn = std::mem::transmute(p);
        let d = f(hwnd) as i32;
        if d > 0 {
            return d;
        }
    }
    default_dpi()
}

/// 供 main 使用的小工具：把窗口句柄塞进 HWND 列表时保持类型清晰。
pub fn hwnd_as_cvoid(h: HWND) -> *mut c_void {
    h as *mut c_void
}
