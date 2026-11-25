//! 中英双语。零依赖做法：一张静态表 + `t("key")`。
//!
//! 不引 i18n 库的理由：界面字符串就三十来条，编译期常量比运行时资源文件更可靠，
//! 也不会出现“换台电脑丢了语言包”的问题——**字符串直接编进 exe**。

use std::sync::atomic::{AtomicU8, Ordering};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Lang {
    Zh = 0,
    En = 1,
}

impl Lang {
    pub fn from_str(s: &str) -> Lang {
        match s.trim().to_ascii_lowercase().as_str() {
            "en" | "english" => Lang::En,
            _ => Lang::Zh,
        }
    }
    pub fn as_str(self) -> &'static str {
        match self {
            Lang::Zh => "zh",
            Lang::En => "en",
        }
    }
    pub fn label(self) -> &'static str {
        match self {
            Lang::Zh => "中文",
            Lang::En => "English",
        }
    }
}

/// 全局语言。用原子量而不是 static mut：读多写少，没必要加锁。
static LANG: AtomicU8 = AtomicU8::new(0);

pub fn set_lang(l: Lang) {
    LANG.store(l as u8, Ordering::Relaxed);
}

pub fn lang() -> Lang {
    if LANG.load(Ordering::Relaxed) == 1 {
        Lang::En
    } else {
        Lang::Zh
    }
}

/// 取一条界面文案。找不到键时回显键名，便于发现遗漏。
pub fn t(key: &'static str) -> &'static str {
    let zh = lang() == Lang::Zh;
    match key {
        // ---- 托盘菜单 ----
        "show_all" => tr("显示所有围栏", "Show all fences"),
        "hide_all" => tr("隐藏所有围栏", "Hide all fences"),
        "dock" => tr("边缘 Dock", "Edge dock"),
        "lock_all" => tr("锁定全部围栏", "Lock all fences"),
        "unlock_all" => tr("解锁全部围栏", "Unlock all fences"),
        "hide_icons" => tr("隐藏桌面图标", "Hide desktop icons"),
        "new_fence" => tr("新建围栏", "New fence"),
        "tidy" => tr("整理桌面到围栏…", "Tidy desktop into fence…"),
        "arrange" => tr("排列桌面图标", "Arrange desktop icons"),
        "undo" => tr("撤销上次整理", "Undo last tidy"),
        "settings" => tr("设置…", "Settings…"),
        "autostart" => tr("开机自启", "Start with Windows"),
        "exit" => tr("退出", "Exit"),

        // ---- 围栏右键 ----
        "expand" => tr("展开围栏", "Expand fence"),
        "collapse" => tr("折叠围栏", "Collapse fence"),
        "lock" => tr("锁定位置", "Lock position"),
        "unlock" => tr("解锁位置", "Unlock position"),
        "exit_fence" => tr("退出桌面围栏", "Exit DesktopFences"),

        // ---- 文件右键 ----
        "open" => tr("打开", "Open"),
        "open_folder" => tr("打开文件夹", "Open folder"),
        "reveal" => tr("在资源管理器中显示", "Show in Explorer"),
        "restore_origin" => tr("放回原位置", "Restore to original location"),
        "restore_desktop" => tr("移回桌面", "Move to desktop"),
        "delete" => tr("删除到回收站", "Delete to Recycle Bin"),

        // ---- 面板内文字 ----
        "empty_title" => tr("空围栏", "Empty fence"),
        "empty_hint" => tr("把文件拖进来，或让它自动归位", "Drop files here, or let them sort themselves"),
        "search" => tr("搜索", "Search"),
        "search_hint" => tr("输入关键字筛选…", "Type to filter…"),
        "no_match" => tr("没有匹配的文件", "No matching files"),

        // ---- 删除围栏 ----
        "fence_props" => tr("围栏属性…", "Fence properties…"),
        "delete_fence" => tr("删除围栏…", "Delete fence…"),
        "delete_fence_title" => tr("删除围栏", "Delete fence"),
        "delete_fence_files" => tr("里面有", "It contains"),
        "delete_fence_unit" => tr("个文件", "files"),
        "delete_fence_empty" => tr("（空围栏）", "(empty fence)"),
        "delete_fence_choose" => tr("请选择文件的去向：", "What should happen to the files?"),
        "delete_fence_yes" => tr(
            "是 —— 把文件放回桌面，然后删除围栏",
            "Yes — move the files back to the desktop, then delete the fence",
        ),
        "delete_fence_no" => tr(
            "否 —— 只删除围栏，文件留在原目录",
            "No — delete the fence only; the files stay where they are",
        ),
        "delete_fence_cancel" => tr("取消 —— 不删除", "Cancel — keep the fence"),
        "delete_fence_done_desktop" => tr(
            "围栏已删除，文件已放回桌面。",
            "Fence deleted; files moved back to the desktop.",
        ),
        "delete_fence_done_keep" => tr("围栏已删除，文件保留在", "Fence deleted; files kept at "),
        "delete_fence_failed" => tr("删除失败。", "Delete failed."),

        // ---- 设置窗口 ----
        "settings_title" => tr("设置", "Settings"),
        "language" => tr("语言", "Language"),
        "general" => tr("常规", "General"),
        "ok" => tr("确定", "OK"),
        "cancel" => tr("取消", "Cancel"),
        "include_dirs" => tr("桌面文件夹也收纳", "Collect desktop folders"),
        "classify_lnk" => tr("快捷方式也收纳（不推荐）", "Collect shortcuts (not recommended)"),

        // ---- 消息 ----
        "app" => tr("桌面围栏", "DesktopFences"),
        "arranged" => tr("已按网格重新排列桌面图标。", "Desktop icons rearranged to grid."),
        "arrange_fail" => tr("排列失败：找不到桌面图标视图。", "Arrange failed: desktop icon view not found."),
        "tidy_confirm" => tr(
            "将把桌面上的文件/文件夹移进围栏。\n\n快捷方式、下载中的文件、正在使用的文件会跳过。\n整理后可以用「撤销上次整理」整体还原。\n\n是否继续？",
            "Move desktop files/folders into the fence.\n\nShortcuts, downloads in progress, and in-use files are skipped.\nYou can undo the whole batch afterwards.\n\nContinue?",
        ),
        "tidy_done" => tr("已整理", "Tidied"),
        "tidy_skip" => tr("跳过", "skipped"),
        "tidy_none" => tr("桌面上没有需要整理的文件。", "Nothing to tidy on the desktop."),
        "undo_done" => tr("已放回", "Restored"),
        "undo_none" => tr("没有可撤销的整理批次", "No tidy batch to undo"),
        "move_fail" => tr("移动失败，文件可能正被占用。", "Move failed; the file may be in use."),
        "restore_fail" => tr("放回原位置失败，目标目录可能不存在或文件被占用。", "Restore failed; the target may be missing or the file in use."),
        "settings_note" => tr("设置已保存", "Settings saved"),
        "first_run_body" => tr(
            "桌面围栏会做两件事：\n\n1. 在 %USERPROFILE%\\DesktopFences\\桌面收纳 下建立一个收纳目录；\n2. 把桌面上新出现的文档、图片、视频等文件移进这个目录。\n\n快捷方式不会被移动；认不出的文件也不会动。\n每次移动都写日志，可以一键撤销。\n\n是否继续？",
            "DesktopFences will:\n\n1. Create a collection folder at %USERPROFILE%\\DesktopFences\\桌面收纳;\n2. Move new documents, images and videos on the desktop into it.\n\nShortcuts are never moved; unknown file types are left alone.\nEvery move is logged and can be undone.\n\nContinue?",
        ),
        "first_run_hide" => tr(
            "要同时隐藏系统桌面图标吗？\n\n隐藏后桌面只显示围栏，随时可以从托盘恢复。",
            "Hide the system desktop icons too?\n\nWith them hidden, only fences show on the desktop. You can restore them from the tray anytime.",
        ),
        "class_fail" => tr("窗口类注册失败，程序无法启动。", "Window class registration failed; cannot start."),
        "ctrl_fail" => tr("主窗口创建失败。", "Failed to create the main window."),
        "no_fence" => tr("一个围栏都没能创建，请检查系统权限。", "No fence could be created; check permissions."),
        "tray_fail" => tr("托盘图标添加失败，程序仍会运行，但没有托盘入口。", "Tray icon failed; the app keeps running without a tray entry."),
        _ => tr(key, key),
    }
}

#[inline]
fn tr(zh: &'static str, en: &'static str) -> &'static str {
    if lang() == Lang::Zh {
        zh
    } else {
        en
    }
}
