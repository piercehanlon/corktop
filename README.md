# corktop

corktop turns a folder into a desktop corkboard. Pin notes, screenshots, links, and files side by side. Pull a thread between two cards to show a connection. When you close the board, your notes are still Markdown files you can open in any editor.

![Four notes on the North Point light board. Red yarn connects the station log to a maintenance slip and a tide table.](docs/media/north-point-board.png)

## Download

| Platform | 1.0.0 RC11 | To launch |
| --- | --- | --- |
| Windows x64 | [Download ZIP](https://github.com/piercehanlon/corktop/releases/download/v1.0.0-rc.11/corktop-1.0.0-windows-x64-136357cad479.zip) | Extract the folder and open `corktop.exe`. The executable is unsigned. |
| Linux x86-64 | [Download tar.gz](https://github.com/piercehanlon/corktop/releases/download/v1.0.0-rc.11/corktop-1.0.0-linux-x64-136357cad479.tar.gz) | Extract the folder and run `./corktop` inside it. This build requires glibc 2.43 or later, libxkbcommon, and an OpenGL-capable X11 or Wayland session. |

The release includes SHA-256 checksums for both downloads. If your Linux distribution has an older glibc, build from source using the instructions below. [Release notes and checksums](https://github.com/piercehanlon/corktop/releases/tag/v1.0.0-rc.11)

## Make your first board

1. Choose **Browse** on first launch and select a folder. Start with an empty one, or choose **Inbox** during setup to pick which existing files to pin.
2. Double-click blank cork to write a note. Put the title on the first line and press **Ctrl+Enter** to save. You can also click blank cork and paste text, a screenshot, or a URL with **Ctrl+V**.
3. Drag a file from your file manager onto the board. Drops default to **Copy**. Choose **Ask**, **Copy**, **Link**, or **Move** in Settings.
4. Drag a card by its body to move it. Drag its pinhead onto another card to connect them. Right-click a strand to cut it.

Click a text card to copy its contents. Double-click to open its file in your editor. For more gestures, open **Settings → Help → Replay tutorial**.

![The four tutorial cards explain copying, pasting and dropping files, pulling yarn, and marking a zone.](docs/media/tutorial.png)

### Watch it in action

- [Write and save a Markdown note (6 seconds, MP4)](docs/media/write-a-note.mp4)
- [Connect two cards with yarn (5 seconds, MP4)](docs/media/connect-cards.mp4)

## Where your work lives

The board folder holds your files. The hidden `.board.toml` file records card positions, strings, and zones. To back up a board, copy the whole folder, including hidden files. Cards added with **Link** still depend on their original files. To keep a file with the board, use **Copy**.

## Build from source

Install Git and rustup, then clone the repository. The build reads the commit ID from Git. The checkout selects Rust 1.90.0 and includes a locked dependency list. [Install Rust](https://rust-lang.org/tools/install/)

```sh
git clone https://github.com/piercehanlon/corktop.git
cd corktop
```

**Linux.** Install a C compiler and linker, pkg-config, and the libxkbcommon and Wayland development packages. On Debian or Ubuntu, the package names are `build-essential`, `pkg-config`, `libxkbcommon-dev`, and `libwayland-dev`. Build and launch from the repository root.

```sh
cargo build --release --locked -p corktop
./target/release/corktop
```

**Windows.** Install the x64 MSVC Rust toolchain, Visual Studio’s **Desktop development with C++** workload, and a Windows SDK. Run the commands below in an x64 Developer PowerShell where `link.exe` and `rc.exe` are on PATH. [Rust’s Windows prerequisites](https://rust-lang.github.io/rustup/installation/windows-msvc.html)

```powershell
cargo build --release --locked -p corktop --target x86_64-pc-windows-msvc
.\target\x86_64-pc-windows-msvc\release\corktop.exe
```

**Cross-build Windows on Linux.** The downloadable ZIP uses the GNU target. Install the MinGW-w64 x86-64 POSIX compiler, Windows headers, and binutils (including `windres`), then run:

```sh
rustup target add x86_64-pc-windows-gnu
export CARGO_TARGET_X86_64_PC_WINDOWS_GNU_LINKER=x86_64-w64-mingw32-gcc-posix
cargo build --release --locked -p corktop --target x86_64-pc-windows-gnu
```

The executable is `target/x86_64-pc-windows-gnu/release/corktop.exe`. The release runbook covers packaging it with its license notices. [Release runbook](docs/reference/operator-runbook.md)

## License

corktop uses the MIT license. Bundled fonts have their own licenses. [MIT license](LICENSE) · [Font notices](THIRD_PARTY_LICENSES.md)
