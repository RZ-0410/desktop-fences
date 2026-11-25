//! 用 windres 把 app.rc（manifest 等资源）编成 COFF 对象再链进 exe。
//! 零第三方依赖，不引 cc/embed-resource 之类。

use std::path::{Path, PathBuf};
use std::process::Command;

fn main() {
    println!("cargo:rerun-if-changed=app.rc");
    println!("cargo:rerun-if-changed=app.manifest");
    println!("cargo:rerun-if-changed=app.ico");

    let out = PathBuf::from(std::env::var("OUT_DIR").expect("OUT_DIR 缺失"));
    let res = out.join("app.res");

    let windres = windres_path();
    // windres 会自己去找名为 `gcc` 的预处理器，还会调 `chcp` 探测代码页。
    // 本机只有带前缀的 x86_64-w64-mingw32-gcc，所以把转发脚本目录和
    // System32 一起塞进子进程 PATH，别动用户的全局环境。
    let mut paths: Vec<PathBuf> = vec![
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tools").join("cc"),
        PathBuf::from(r"E:\35087\tools\mingw\mingw64\bin"),
    ];
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

/// 优先环境变量，其次本机便携 mingw，最后靠 PATH。
fn windres_path() -> PathBuf {
    if let Ok(p) = std::env::var("WINDRES") {
        return PathBuf::from(p);
    }
    let portable = Path::new(r"E:\35087\tools\mingw\mingw64\bin\windres.exe");
    if portable.exists() {
        return portable.to_path_buf();
    }
    PathBuf::from("windres.exe")
}
