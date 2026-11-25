use serde_json::Value;
use std::process::{Command, Output};

fn invoke(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_corktop"))
        .args(args)
        .output()
        .unwrap()
}

#[test]
fn reports_identity_and_checks_storage_offline() {
    for (flag, field, expected) in [
        ("--identity", "commit", env!("CORKTOP_SHA")),
        ("--health", "health", "pass"),
        ("--ready", "readiness", "pass"),
    ] {
        let out = invoke(&[flag]);
        assert!(out.status.success());
        let json: Value = serde_json::from_slice(&out.stdout).unwrap();
        assert_eq!(json[field], expected);
    }
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("scratch");
    let first = invoke(&["--probe", root.to_str().unwrap()]);
    let second = invoke(&["--probe", root.to_str().unwrap()]);
    assert!(first.status.success() && second.status.success());
    assert_eq!(first.stdout, second.stdout);
}

#[test]
fn rejects_extra_arguments_without_echoing_paths() {
    let out = invoke(&["--ready", "sensitive-example"]);
    assert!(!out.status.success());
    let log: Value = serde_json::from_slice(&out.stderr).unwrap();
    assert_eq!(log["level"], "error");
    assert!(!log.to_string().contains("sensitive-example"));
}
