//! 用 windres 把 app.rc（manifest 等资源）编成 COFF 对象再链进 exe。
//! 零第三方依赖，不引 cc/embed-resource 之类。
//!
//! 可移植性约定：**源码里不出现任何本机绝对路径**。
//! 工具链位置一律由 PATH / 环境变量（WINDRES、CC）决定。

use std::path::{Path, PathBuf};
use std::process::Command;

fn main() {
    println!("cargo:rerun-if-changed=app.rc");
    println!("cargo:rerun-if-changed=app.manifest");
    println!("cargo:rerun-if-changed=app.ico");

    let out = PathBuf::from(std::env::var("OUT_DIR").expect("OUT_DIR 缺失"));
    let res = out.join("app.res");

    let windres = windres_path();
    let gcc = gcc_path_for(&windres);

    // windres 会自己去找名为 `gcc` 的预处理器，但很多工具链只提供带前缀的
    // x86_64-w64-mingw32-gcc。这里在 OUT_DIR 里生成一个转发脚本，
    // 只影响本次构建的子进程 PATH，不碰用户的全局环境。
    let shim_dir = out.join("shim");
    let _ = std::fs::create_dir_all(&shim_dir);
    write_gcc_shim(&shim_dir, &gcc);

    let mut paths: Vec<PathBuf> = vec![shim_dir.clone()];
    if let Some(bin) = windres.parent() {
        if bin.components().count() > 1 {
            paths.push(bin.to_path_buf());
        }
    }
    let sysroot = std::env::var("SystemRoot").unwrap_or_else(|_| r"C:\Windows".to_string());
    paths.push(PathBuf::from(sysroot).join("System32"));
    if let Some(p) = std::env::var_os("PATH") {
        paths.extend(std::env::split_paths(&p));
    }
    let child_path = std::env::join_paths(paths).unwrap_or_default();

    let status = Command::new(&windres)
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .env("PATH", &child_path)
        .args(["-I", ".", "-O", "coff", "app.rc", "-o"])
        .arg(&res)
        .status();

    match status {
        Ok(s) if s.success() => {}
        Ok(s) => panic!("windres 失败，退出码 {:?}（{}）", s.code(), windres.display()),
        Err(e) => panic!("无法执行 windres（{}）：{}", windres.display(), e),
    }

    println!("cargo:rustc-link-arg={}", res.display());
}

/// windres 查找顺序：WINDRES 环境变量 → 带前缀的名字 → 裸名。
/// 一律返回**绝对路径**：否则拿不到同目录的 gcc，无法生成转发脚本。
fn windres_path() -> PathBuf {
    if let Ok(p) = std::env::var("WINDRES") {
        return PathBuf::from(p);
    }
    for name in [
        "x86_64-w64-mingw32-windres.exe",
        "x86_64-w64-mingw32-windres",
        "windres.exe",
        "windres",
    ] {
        if let Some(p) = resolve_in_path(name) {
            return p;
        }
    }
    PathBuf::from("windres.exe")
}

/// 与 windres 同目录的 gcc（通常带同样的前缀），找不到再从 PATH 解析。
fn gcc_path_for(windres: &Path) -> PathBuf {
    if let Ok(p) = std::env::var("CC") {
        return PathBuf::from(p);
    }
    if let Some(dir) = windres.parent() {
        for name in ["x86_64-w64-mingw32-gcc.exe", "gcc.exe"] {
            let c = dir.join(name);
            if c.exists() {
                return c;
            }
        }
    }
    resolve_in_path("gcc.exe").unwrap_or_else(|| PathBuf::from("gcc.exe"))
}

/// 在 PATH 里找一个可执行文件，返回绝对路径。
fn resolve_in_path(name: &str) -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    for dir in std::env::split_paths(&path) {
        if dir.as_os_str().is_empty() {
            continue;
        }
        let candidate = dir.join(name);
        if candidate.is_file() {
            return Some(candidate);
        }
    }
    None
}

/// 只有拿到 gcc 的绝对路径时才生成转发脚本；
/// 若只是裸名（PATH 上有同名 gcc），生成脚本反而会自指递归。
fn write_gcc_shim(dir: &Path, gcc: &Path) {
    if !gcc.is_absolute() {
        return;
    }
    // 保持 ASCII：cmd.exe 按 OEM 代码页读 .cmd，UTF-8 中文注释会乱码
    let content = format!("@echo off\r\n\"{}\" %*\r\n", gcc.display());
    let _ = std::fs::write(dir.join("gcc.cmd"), content);
}
