use std::process::Command;

fn git(args: &[&str]) -> String {
    let output = Command::new("git").args(args).output().expect("run git");
    assert!(output.status.success(), "git {args:?} failed");
    String::from_utf8(output.stdout).unwrap().trim().into()
}

fn watch_git_path(path: &str) {
    let path = git(&["rev-parse", "--path-format=absolute", "--git-path", path]);
    println!("cargo:rerun-if-changed={path}");
}

fn windows_icon() {
    use std::{env, path::PathBuf};
    println!("cargo:rerun-if-changed=assets/corktop.ico");
    println!("cargo:rerun-if-changed=assets/corktop.rc");
    let out = PathBuf::from(env::var_os("OUT_DIR").unwrap());
    let (object, mut compiler) = if env::var("CARGO_CFG_TARGET_ENV").as_deref() == Ok("msvc") {
        let path = out.join("corktop.res");
        let mut rc = Command::new("rc.exe");
        rc.arg("/nologo")
            .arg("/fo")
            .arg(&path)
            .arg("assets/corktop.rc");
        (path, rc)
    } else {
        let path = out.join("corktop-icon.o");
        let mut rc = Command::new("x86_64-w64-mingw32-windres");
        rc.args([
            "--preprocessor",
            "x86_64-w64-mingw32-gcc-posix",
            "--preprocessor-arg=-E",
            "--preprocessor-arg=-xc",
            "-O",
            "coff",
            "-i",
            "assets/corktop.rc",
            "-o",
        ])
        .arg(&path);
        (path, rc)
    };
    assert!(
        compiler
            .status()
            .expect("compile the Windows icon resource")
            .success(),
        "Windows icon resource compilation failed"
    );
    println!("cargo:rustc-link-arg-bin=corktop={}", object.display());
}

fn main() {
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        windows_icon();
    }
    watch_git_path("HEAD");
    let head = git(&["rev-parse", "--symbolic-full-name", "HEAD"]);
    if head.starts_with("refs/") {
        watch_git_path(&head);
        watch_git_path("packed-refs");
    }
    println!(
        "cargo:rustc-env=CORKTOP_SHA={}",
        git(&["rev-parse", "HEAD"])
    );
    println!(
        "cargo:rustc-env=CORKTOP_TARGET={}",
        std::env::var("TARGET").unwrap()
    );
}
