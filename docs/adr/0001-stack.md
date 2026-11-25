# 0001 · Native Rust desktop

**Status** Accepted for Pilot.

**Context** One offline binary must run as a standard window on each desktop. Cards remain ordinary files.

**Options** A webview needs a browser runtime. Direct platform APIs duplicate the window and input code. A native Rust window library serves both platforms.

**Decision** Use Rust 1.90.0, edition 2024, with Cargo, rustfmt, and Clippy from that toolchain. Use eframe 0.32.3 for a native event-driven window, with its default fonts, OpenGL renderer, X11 and Wayland support. Use toml 0.9.8 for manifest parsing and writing, and serde_json 1.0.145 for structured output. Use GNU Make 4.3 for local tasks on Linux. Use Gitleaks 8.24.3 for secret scanning and cargo-deny 0.22.1 for license and advisory checks. Acquire Rust crates from crates.io. Use the official Rust distribution for the compiler. Keep Cargo.lock under version control. The project toolchain lock records exact tool versions and setup checks each one.

**Consequences** Windowing depends on the native desktop and graphics driver. Keep rendering outside the board model so the window library can be replaced. Ship required third-party notices with each binary. Build Windows binaries on Windows and Linux binaries on Linux. No hosted builds or paid services are approved.
