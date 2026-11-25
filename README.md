# 桌面围栏 DesktopFences

把桌面上的文件按类型自动收进「围栏」面板，桌面只留干净的格子。零第三方依赖的 Windows 桌面工具。

- **自动归位**：新落地的文档/图片/视频/压缩包/源码，1.4 秒内被移进围栏（同盘改名，不复制）
- **一次整理**：右键一键把桌面现有文件收进来，整批可撤销
- **文件夹与快捷方式可控**：文件夹可选收纳；快捷方式永远留在桌面（可配置）
- **搜索**：面板内按文件名筛选
- **撤销**：每个移动都写日志，支持单个放回原位置、整批撤销
- **中英双语**：设置里切换，即时生效
- **图标排列**：一键把桌面图标按网格排整齐（只动位置，不碰文件）

## 在其他电脑上运行

**不需要安装、不需要运行库、不需要联网。** 把 `DesktopFences.exe` 拷到目标机器，双击即可。

| 要求 | 说明 |
|---|---|
| 系统 | Windows 10（1607 及以上）/ Windows 11，**x64** |
| 安装 | 无需 |
| 运行库 | **无需**（C 运行时已静态链接） |
| 权限 | 普通用户即可；若要收纳 `C:\Users\Public\Desktop` 下的快捷方式，需管理员 |
| 首次运行 | 自动创建 `%USERPROFILE%\DesktopFences\桌面收纳` 与 `%APPDATA%\DesktopFences\config.ini` |

换机器、换用户名都不影响：配置里的绝对路径失效时会按围栏名自动重建。

### DLL 依赖

用 `objdump -p DesktopFences.exe` 可复现下表。**全部是 Windows 系统自带的 DLL，没有任何第三方或 mingw 运行时。**

| DLL | 来源 | 用途 |
|---|---|---|
| `KERNEL32.dll` | Windows 自带 | 进程/线程/文件/内存基础 API |
| `USER32.dll` | Windows 自带 | 窗口、消息、鼠标键盘、托盘 |
| `GDI32.dll` | Windows 自带 | 位图、字体、绘制 |
| `SHELL32.dll` | Windows 自带 | 文件图标、回收站、资源管理器 |
| `ole32.dll` | Windows 自带 | COM（图标提取、拖放） |
| `ADVAPI32.dll` | Windows 自带 | 注册表（开机自启） |
| `ntdll.dll` | Windows 自带 | 系统底层 |
| `WS2_32.dll` | Windows 自带 | Winsock（未实际使用，来自 CRT 引用） |
| `api-ms-win-crt-*.dll`（8 个） | Windows 10+ 自带（UCRT） | C 运行时 API 集 |
| `api-ms-win-core-synch-l1-2-0.dll` | Windows 10+ 自带 | 同步原语 |
| `bcryptprimitives.dll` | Windows 10+ 自带 | 加密基元（随机数） |

> **为什么没有 `libgcc_s_seh-1.dll`、`libwinpthread-1.dll`、`msvcr100.dll`？**
> 构建时开了 `-C target-feature=+crt-static`，把 C 运行时静态链进了 exe。
> 这正是「拷一个 exe 就能跑」的原因。

Windows 7 / 8.1 上 `api-ms-win-crt-*` 需要装一次 [KB2999226](https://support.microsoft.com/help/2999226)（Windows Update 通常已带）。目标是 Windows 10+ 时无须关心。

## 从源码构建

### 工具链

```bash
# 1. Rust，GNU 工具链（不是 MSVC）
rustup toolchain install stable-x86_64-pc-windows-gnu
rustup default stable-x86_64-pc-windows-gnu
rustup target add x86_64-pc-windows-gnu

# 2. MinGW-w64 的 gcc（用于链接）
#    MSYS2:  pacman -S mingw-w64-x86_64-gcc
#    便携版: 下载解压后把 mingw64\bin 加进 PATH
```

### 编译

```bash
cargo build --release      # 产物在 target/x86_64-pc-windows-gnu/release/DesktopFences.exe
```

如果你想让 exe 不依赖 PATH 里的 gcc，把 `.cargo/config.toml` 里的 `linker` / `ar` 改成绝对路径：

```toml
linker = "D:/tools/mingw/mingw64/bin/x86_64-w64-mingw32-gcc.exe"
ar = "D:/tools/mingw/mingw64/x86_64-w64-mingw32/bin/ar.exe"
```

### 确认产物可移植

```bash
objdump -p target/x86_64-pc-windows-gnu/release/DesktopFences.exe | grep "DLL Name"
```

输出应当只有上表那些系统 DLL。如果冒出 `libgcc` / `libwinpthread` / `msvcr`，说明 `crt-static` 没生效，检查 `.cargo/config.toml`。

## 目录结构

```
src/
  main.rs        程序入口、消息循环、菜单、整理/撤销/删除围栏
  fence_ui.rs    围栏窗口：自绘面板、图标网格、搜索、拖拽
  classify.rs    桌面监听（ReadDirectoryChangesW）与文件归位
  config.rs      配置读写、围栏定义、路径重映射
  icons.rs       图标缓存（不重复 stat，元数据来自目录扫描）
  journal.rs     移动日志，支持撤销
  shell.rs       桌面图标显隐、全屏检测、ShellExecute
  gfx.rs         分层窗口画布、圆角矩形、文字排版
  winapi.rs      零依赖的 Win32 绑定（只声明用到的函数）
  i18n.rs        中英双语表（编进 exe，不引外部资源）
  settings_ui.rs 设置窗口（语言、开机自启、收纳开关）
  tray.rs        托盘图标与菜单
  drop.rs        文件拖入（WM_DROPFILES / OLE）
build.rs         嵌入图标与 manifest
```

## 设计要点

**为什么零依赖？** 只用 Win32 API，不引任何 crate。好处是产物 0.48 MB、依赖面小、换机器不带运行时；代价是 `winapi.rs` 里手写了 2000 行声明。

**为什么是「移动」不是「复制」？** 同盘走 `rename`，只改目录项、不读写文件内容，大文件也是秒完。跨盘时退化为「复制 + 删原件」，文件夹跨盘直接跳过（不做递归复制）。

**为什么快捷方式不进围栏？** 快捷方式是应用入口，收起来会打断使用习惯。桌面的排序整理用「排列桌面图标」，只动位置不碰文件。

**空闲时读盘吗？** 几乎为零。目录扫描用一次 `FindFirstFileW` 拿全部元数据（不是 N 次 stat），图标缓存不重复查文件，桌面监听是事件驱动不轮询。实测空闲 15 秒 CPU 耗时 0.000 秒。

## 配置

`%APPDATA%\DesktopFences\config.ini`，可手改，改完重启生效。关键项：

| 键 | 默认 | 说明 |
|---|---|---|
| `lang` | `zh` | 界面语言 `zh` / `en` |
| `classify_lnk` | `0` | 快捷方式是否收纳（建议保持 0） |
| `include_dirs` | `1` | 桌面文件夹是否收纳 |
| `hide_desktop_icons` | `0` | 是否隐藏系统桌面图标 |
| `dry_run` | `0` | 试运行：只写日志不移动文件 |
| `sort` | `mtime` | 围栏排序：`mtime` / `name` / `ext` |

## 许可

未指定。仅供学习与个人使用。
