#![cfg_attr(target_os = "windows", windows_subsystem = "windows")]

mod board_window;
use board_window::BoardWindow;
use eframe::egui;
use std::process::ExitCode;

const VERSION: &str = env!("CARGO_PKG_VERSION");

struct VersionWindow;

impl eframe::App for VersionWindow {
    fn update(&mut self, ctx: &egui::Context, _: &mut eframe::Frame) {
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.centered_and_justified(|ui| ui.label(VERSION));
        });
    }
}

fn failure(message: &str) -> ExitCode {
    let utc = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    eprintln!(
        "{}",
        serde_json::json!({"utc_unix_seconds":utc,"level":"error","component":"desktop","message":message,"build_sha":env!("CORKTOP_SHA")})
    );
    ExitCode::FAILURE
}

fn main() -> ExitCode {
    let mut args = std::env::args_os().skip(1);
    let command = args.next();
    let parameter = args.next();
    if args.next().is_some() {
        return failure("unexpected arguments");
    }
    match (command, parameter) {
        (Some(flag), None) if flag == "--version-window" => {
            let options = eframe::NativeOptions {
                viewport: egui::ViewportBuilder::default()
                    .with_decorations(false)
                    .with_inner_size([320.0, 160.0]),
                ..Default::default()
            };
            if eframe::run_native(
                "corktop",
                options,
                Box::new(|_| Ok(Box::new(VersionWindow))),
            )
            .is_err()
            {
                return failure("window initialization failed");
            }
        }
        (None, None) => return board_window(None),
        (Some(flag), Some(path)) if flag == "--board" => return board_window(Some(path)),
        (Some(flag), None) if flag == "--version" => println!("{VERSION}"),
        (Some(flag), None) if flag == "--identity" => println!(
            "{}",
            serde_json::json!({
                "version": VERSION,
                "commit": env!("CORKTOP_SHA"),
                "target": env!("CORKTOP_TARGET"),
                "manifest_version": corktop_board::MANIFEST_VERSION,
            })
        ),
        (Some(flag), None) if flag == "--health" => {
            if std::env::current_exe().and_then(std::fs::metadata).is_err() {
                return failure("executable unavailable");
            }
            println!(r#"{{"health":"pass"}}"#);
        }
        (Some(flag), None) if flag == "--ready" => {
            let check =
                tempfile::tempdir().and_then(|dir| corktop_board::probe(&dir.path().join("board")));
            if check.is_err() {
                return failure("scratch storage check failed");
            }
            println!(r#"{{"readiness":"pass"}}"#);
        }
        (Some(flag), Some(path)) if flag == "--probe" => {
            if corktop_board::probe(std::path::Path::new(&path)).is_err() {
                return failure("synthetic persistence probe failed");
            }
            println!(
                "{}",
                serde_json::json!({"marker":corktop_board::MARKER,"schema_version":corktop_board::MANIFEST_VERSION})
            );
        }
        _ => {
            failure(
                "expected --board PATH, --version-window, --version, --identity, --health, --ready, --probe PATH, or no arguments",
            );
            return ExitCode::from(2);
        }
    }
    ExitCode::SUCCESS
}

fn board_window(path: Option<std::ffi::OsString>) -> ExitCode {
    let mut app = BoardWindow::default();
    if let Some(path) = path {
        app.open(std::path::Path::new(&path));
    }
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_decorations(false)
            .with_inner_size([780.0, 540.0]),
        ..Default::default()
    };
    if eframe::run_native(
        "corktop",
        options,
        Box::new(move |_cc| {
            #[cfg(windows)]
            {
                // Use the same user-configured interval for recognition and deferred copy.
                let millis = unsafe {
                    windows_sys::Win32::UI::Input::KeyboardAndMouse::GetDoubleClickTime()
                };
                _cc.egui_ctx.options_mut(|o| {
                    o.input_options.max_double_click_delay = f64::from(millis) / 1000.0
                });
            }
            Ok(Box::new(app))
        }),
    )
    .is_err()
    {
        return failure("window initialization failed");
    }
    ExitCode::SUCCESS
}
