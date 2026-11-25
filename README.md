<div align="center">

# 桌面围栏 DesktopFences

**把桌面上的文件按类型自动收进「围栏」面板，桌面只留干净的格子。**
**Sort desktop files into fence panels by type — keep your desktop clean.**

零第三方依赖 · 单文件 0.48 MB · 拷贝即用
Zero third-party dependencies · Single 0.48 MB exe · Just copy and run

[下载 Download](#-下载download) · [使用 Usage](#-使用说明usage) · [构建 Build](#-从源码构建build-from-source)

</div>

---

## 📥 下载（Download）

### 直接下载成品（Download the ready-made exe）

1. 打开本仓库的 **[Releases](../../releases)** 页面
2. 在最新版本（Latest）下找到 `DesktopFences.exe`
3. 点击下载

Go to the **[Releases](../../releases)** page → download `DesktopFences.exe` from the latest release.

### 在别的电脑上运行（Run on another PC）

**不需要安装、不需要运行库、不需要联网。** 把 `DesktopFences.exe` 拷到目标机器，双击即可。

**No installation, no runtime, no internet needed.** Copy `DesktopFences.exe` to the target machine and double-click it.

| | |
|---|---|
| 系统 OS | Windows 10（1607+）/ Windows 11，**x64** |
| 安装 Install | 不需要 None |
| 运行库 Runtime | 不需要 None（C 运行时已静态链接 / statically linked） |
| 权限 Privileges | 普通用户即可；收纳公共桌面需管理员 Normal user; admin only for the public desktop |
| 首次运行 First run | 自动创建 `%USERPROFILE%\DesktopFences\桌面收纳` 与配置文件 Config is created automatically |

换机器、换用户名都不影响：配置里的绝对路径失效时会按围栏名自动重建。
Moving to another machine or user account is fine — stale paths in the config are rebuilt automatically.

---

## 📖 使用说明（Usage）

### 快速上手（Quick start）

1. 双击 `DesktopFences.exe` 启动，屏幕右侧出现一个围栏面板
2. 新建一个 Word/PDF/图片到桌面，**1.4 秒内**它会自动移进围栏
3. 想把已有的桌面文件也收进去：右键围栏 → **整理桌面到围栏…**
4. 收错文件了：托盘图标 → **撤销上次整理**，整批还原

Double-click `DesktopFences.exe` — a fence panel appears on the right. New files on the desktop are moved into it within 1.4 seconds. Right-click the fence → **Tidy desktop into fence…** to collect existing files. **Undo last tidy** restores the whole batch.

### 右键菜单（Context menu）

| 围栏面板（Fence panel） | 文件（File） |
|---|---|
| 折叠/展开围栏 Collapse/Expand | 打开 Open |
| 锁定位置 Lock position | 在资源管理器中显示 Show in Explorer |
| 排列桌面图标 Arrange desktop icons | 放回原位置 Restore to original location |
| 整理桌面到围栏… Tidy desktop into fence… | 移回桌面 Move to desktop |
| 新建围栏 New fence | 删除到回收站 Delete to Recycle Bin |
| **删除围栏… Delete fence…** | |
| 隐藏所有围栏 Hide all fences | |
| 退出桌面围栏 Exit | |

### 托盘菜单（Tray menu）

右键屏幕右下角的托盘图标：显示/隐藏围栏、边缘 Dock、隐藏桌面图标、整理、撤销、设置、开机自启、退出。

Right-click the tray icon: show/hide fences, edge dock, hide desktop icons, tidy, undo, settings, autostart, exit.

### 搜索（Search）

点击围栏标题栏右侧的 **搜索**，输入关键字即时筛选文件名，按 `Esc` 关闭。

Click **Search** on the title bar to filter by file name; press `Esc` to close.

### 设置（Settings）

托盘 → **设置…**：界面语言（中文 / English）、开机自启、隐藏桌面图标、是否收纳文件夹。

Tray → **Settings…**: language (中文 / English), autostart, hide desktop icons, collect folders.

### 删除围栏（Delete a fence）

右键围栏 → **删除围栏…**，三个选项，**文件绝不静默丢失**：

Right-click a fence → **Delete fence…**. Three choices — files are **never silently lost**:

| 选项 Choice | 行为 Action |
|---|---|
| 是 Yes | 文件放回桌面，然后删除围栏 Move files back to the desktop, then delete |
| 否 No | 只删面板，文件留在原目录 Panel only; files stay in place |
| 取消 Cancel | 不删 Keep it |

---

## ✨ 功能（Features）

| 中文 | English |
|---|---|
| 自动归位：新文件按扩展名收进围栏，同盘 rename 不复制 | Auto-sort: new files move in by extension (same-volume rename, not copy) |
| 一次整理现有桌面，整批可撤销 | One-click tidy with full undo |
| 单个文件放回原位置 | Restore a single file to where it came from |
| 文件夹可选收纳，快捷方式永远留在桌面 | Optional folder collection; shortcuts always stay on the desktop |
| 面板内搜索、拖拽、多围栏、折叠、锁定 | Search, drag & drop, multiple fences, collapse, lock |
| 中英双语即时切换 | Bilingual UI, switch instantly |
| 空闲零磁盘读写 | Zero disk I/O when idle |
| 图标网格排列（只动位置不碰文件） | Icon grid arrangement (positions only) |

---

## 📦 DLL 依赖（DLL Dependencies）

用 `objdump -p DesktopFences.exe` 可复现下表。**全部是 Windows 系统自带 DLL，没有任何第三方或 mingw 运行时。**

Reproduce with `objdump -p DesktopFences.exe`. **All are Windows system DLLs — no third-party or mingw runtime.**

| DLL | 来源 Source | 用途 Purpose |
|---|---|---|
| `KERNEL32.dll` | Windows 自带 | 进程/线程/文件/内存 |
| `USER32.dll` | Windows 自带 | 窗口、消息、托盘 |
| `GDI32.dll` | Windows 自带 | 位图、字体、绘制 |
| `SHELL32.dll` | Windows 自带 | 文件图标、回收站 |
| `ole32.dll` | Windows 自带 | COM（图标、拖放） |
| `ADVAPI32.dll` | Windows 自带 | 注册表（开机自启） |
| `ntdll.dll` / `WS2_32.dll` | Windows 自带 | 系统底层 / Winsock |
| `api-ms-win-crt-*.dll`（8 个） | Windows 10+ 自带 UCRT | C 运行时 API 集 |
| `api-ms-win-core-synch-l1-2-0.dll` | Windows 10+ 自带 | 同步原语 |
| `bcryptprimitives.dll` | Windows 10+ 自带 | 加密基元 |

> **为什么没有 `libgcc_s_seh-1.dll` / `libwinpthread-1.dll` / `msvcr100.dll`？**
> 构建时开了 `-C target-feature=+crt-static`，C 运行时已静态链进 exe。这正是「拷一个 exe 就能跑」的原因。
>
> **Why are those missing?** The build uses `-C target-feature=+crt-static`, so the C runtime is compiled in. That is exactly why a single exe runs anywhere.

Windows 7 / 8.1 上 `api-ms-win-crt-*` 需装一次 [KB2999226](https://support.microsoft.com/help/2999226)（Windows Update 通常已带）。目标是 Windows 10+ 时无须关心。
On Windows 7/8.1 you may need [KB2999226](https://support.microsoft.com/help/2999226). On Windows 10+ it is already there.

---

## 🛠 从源码构建（Build from Source）

### 工具链（Toolchain）

```bash
# 1. Rust，GNU 工具链（不是 MSVC）— GNU toolchain, not MSVC
rustup toolchain install stable-x86_64-pc-windows-gnu
rustup default stable-x86_64-pc-windows-gnu
rustup target add x86_64-pc-windows-gnu

# 2. MinGW-w64 的 gcc — MinGW-w64 gcc
#    MSYS2:  pacman -S mingw-w64-x86_64-gcc
#    便携版: 解压后把 mingw64\bin 加进 PATH — portable: add mingw64\bin to PATH
```

### 编译（Build）

```bash
cargo build --release
# 产物 Output: target/x86_64-pc-windows-gnu/release/DesktopFences.exe
```

不想依赖 PATH 里的 gcc，就把 `.cargo/config.toml` 里的 `linker`/`ar` 改成绝对路径：
To avoid depending on PATH, set absolute paths in `.cargo/config.toml`:

```toml
linker = "D:/tools/mingw/mingw64/bin/x86_64-w64-mingw32-gcc.exe"
ar = "D:/tools/mingw/mingw64/x86_64-w64-mingw32/bin/ar.exe"
```

### 确认产物可移植（Verify portability）

```bash
objdump -p target/x86_64-pc-windows-gnu/release/DesktopFences.exe | grep "DLL Name"
```

输出应当只有上表那些系统 DLL。若冒出 `libgcc`/`libwinpthread`/`msvcr`，说明 `crt-static` 没生效。
The output must contain only the system DLLs above. If you see `libgcc` / `libwinpthread` / `msvcr`, `crt-static` did not take effect.

---

## 🗂 目录结构（Project Layout）

```
src/
  main.rs        入口、消息循环、菜单、整理/撤销/删除围栏   entry, message loop, menus
  fence_ui.rs    围栏窗口：自绘面板、图标网格、搜索、拖拽    fence window (self-drawn)
  classify.rs    桌面监听与文件归位                          desktop watcher & sorting
  config.rs      配置读写、围栏定义、路径重映射              config, fence defs, path remap
  icons.rs       图标缓存（不重复 stat）                     icon cache
  journal.rs     移动日志，支持撤销                          move journal + undo
  shell.rs       桌面图标显隐、ShellExecute                  shell helpers
  gfx.rs         分层窗口画布、圆角矩形、文字排版            layered-window canvas
  winapi.rs      零依赖 Win32 绑定（只声明用到的函数）       hand-written Win32 bindings
  i18n.rs        中英双语表（编进 exe）                      bilingual strings (compiled in)
  settings_ui.rs 设置窗口                                    settings window
  tray.rs        托盘图标与菜单                              tray icon & menu
  drop.rs        文件拖入                                    file drag & drop
build.rs         嵌入图标与 manifest                         embed icon + manifest
```

---

## ⚙️ 配置（Configuration）

`%APPDATA%\DesktopFences\config.ini`，可手改，重启生效。Edit by hand; restart to apply.

| 键 Key | 默认 Default | 说明 Meaning |
|---|---|---|
| `lang` | `zh` | 界面语言 `zh` / `en` |
| `classify_lnk` | `0` | 快捷方式是否收纳（建议保持 0） Collect shortcuts (keep 0) |
| `include_dirs` | `1` | 桌面文件夹是否收纳 Collect folders |
| `hide_desktop_icons` | `0` | 隐藏系统桌面图标 Hide desktop icons |
| `dry_run` | `0` | 试运行：只写日志不移动 Dry run: log only |
| `sort` | `mtime` | 排序 `mtime` / `name` / `ext` |

---

## 💡 设计要点（Design Notes）

**为什么零依赖？** 只用 Win32 API，不引任何 crate。好处是产物 0.48 MB、依赖面小、换机器不带运行时；代价是 `winapi.rs` 手写了约 1000 行声明。
*Why zero dependencies?* Only Win32 API, no crates — small binary, tiny attack surface, no runtime to carry. The cost: ~1000 lines of hand-written bindings.

**为什么是「移动」不是「复制」？** 同盘走 `rename`，只改目录项、不读写文件内容。跨盘时退化为「复制 + 删原件」，文件夹跨盘直接跳过。
*Move, not copy:* same-volume `rename` touches only the directory entry. Cross-volume falls back to copy+delete; cross-volume folders are skipped.

**空闲时读盘吗？** 几乎为零。目录扫描用一次 `FindFirstFileW` 拿全部元数据，图标缓存不重复查文件，桌面监听是事件驱动。实测空闲 15 秒 CPU 0.000 秒。
*Idle disk I/O?* Essentially zero — one `FindFirstFileW` per scan, event-driven watcher. Measured 0.000 s CPU over 15 idle seconds.

---

## 📄 许可（License）

未指定，仅供学习与个人使用。
Unspecified. For learning and personal use only.
