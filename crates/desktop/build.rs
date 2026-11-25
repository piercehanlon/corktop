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

fn main() {
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
