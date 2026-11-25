#![cfg_attr(target_os = "windows", windows_subsystem = "windows")]

mod board_window;
mod shell;
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
                    .with_icon(shell::app_icon())
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
        (None, None) => return board_window(None, None, false),
        (Some(flag), Some(path)) if flag == "--board" => {
            return board_window(Some(path), None, false);
        }
        (Some(flag), path) if flag == "--desktop" || flag == "--window" => {
            return board_window(path, Some(flag == "--desktop"), false);
        }
        (Some(path), None) if !path.to_string_lossy().starts_with('-') => {
            return board_window(Some(path), None, false);
        }
        (Some(flag), path) if flag == "--panel" => return board_window(path, Some(false), true),
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

fn board_window(path: Option<std::ffi::OsString>, mode: Option<bool>, panel: bool) -> ExitCode {
    let path = match path.map(std::path::absolute).transpose() {
        Ok(path) => path,
        Err(_) => return failure("invalid board path"),
    };
    let control = if panel {
        shell::control::Control::local()
    } else {
        match shell::control::start(shell::control::Launch::Open {
            path: path.clone(),
            desktop: mode,
        }) {
            Ok(Some(c)) => c,
            Ok(None) => return ExitCode::SUCCESS,
            Err(e) => {
                eprintln!("Launch routing: {e}");
                return failure("could not route launch");
            }
        }
    };
    let mut state = match shell::MachineState::load() {
        Ok(s) => s,
        Err(_) => return failure("could not read local window state"),
    };
    if let Some(mode) = mode {
        state.desktop = mode;
    }
    let folder = path.or_else(|| state.folder.clone().filter(|f| !panel && f.is_dir()));
    let mut app = BoardWindow::default();
    app.machine = state;
    app.panel = panel;
    app.control = Some(control);
    if let Some(path) = folder {
        app.open(&path);
    }
    // Keep the board and its lock alive when Wayland requires a new surface role.
    let app = std::rc::Rc::new(std::cell::RefCell::new(app));
    #[cfg(target_os = "linux")]
    let mut wayland_events = None;
    // Linux can recreate the surface when switching between desktop and standard roles.
    #[cfg_attr(not(target_os = "linux"), allow(clippy::never_loop))]
    loop {
        #[cfg(target_os = "linux")]
        if app.borrow().machine.desktop && std::env::var_os("WAYLAND_DISPLAY").is_some() {
            let mut board = app.borrow_mut();
            match shell::wayland::run(&mut board) {
                Ok(shell::wayland::Exit::Quit) => {
                    eframe::App::on_exit(&mut *board, None);
                    return ExitCode::SUCCESS;
                }
                Ok(shell::wayland::Exit::Standard) => board.machine.desktop = false,
                Err(e) => {
                    board.machine.desktop = false;
                    board.layer_unavailable(e);
                }
            }
        }
        let options = eframe::NativeOptions {
            viewport: app.borrow().machine.viewport(),
            ..Default::default()
        };
        let window = app.clone();
        let creator: eframe::AppCreator = Box::new(move |cc| {
            #[cfg(windows)]
            {
                let millis = unsafe {
                    windows_sys::Win32::UI::Input::KeyboardAndMouse::GetDoubleClickTime()
                };
                cc.egui_ctx.options_mut(|o| {
                    o.input_options.max_double_click_delay = f64::from(millis) / 1000.0
                });
            }
            window.borrow_mut().attach(&cc.egui_ctx);
            Ok(Box::new(StandardWindow(window)))
        });
        #[cfg(target_os = "linux")]
        let result = if std::env::var_os("WAYLAND_DISPLAY").is_some() {
            shell::wayland::standard(&mut wayland_events, options, creator)
        } else {
            eframe::run_native("corktop", options, creator)
        };
        #[cfg(not(target_os = "linux"))]
        let result = eframe::run_native("corktop", options, creator);
        if result.is_err() {
            return failure("window initialization failed");
        }
        #[cfg(target_os = "linux")]
        if std::mem::take(&mut app.borrow_mut().change_host)
            && app.borrow_mut().wait_for_visibility()
        {
            continue;
        }
        return ExitCode::SUCCESS;
    }
}

struct StandardWindow(std::rc::Rc<std::cell::RefCell<BoardWindow>>);
impl eframe::App for StandardWindow {
    fn update(&mut self, ctx: &eframe::egui::Context, frame: &mut eframe::Frame) {
        self.0.borrow_mut().update(ctx, frame);
    }
    fn on_exit(&mut self, gl: Option<&eframe::glow::Context>) {
        self.0.borrow_mut().on_exit(gl);
    }
}
