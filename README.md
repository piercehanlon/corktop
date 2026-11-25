# corktop
Lightweight, portable spatial scratchpad and pin board. Or that's what they want you to think.

## Open a folder board

Launch Corktop, enter the path to an empty directory or an existing Corktop board, and choose **Open folder**. From a checkout, `cargo run --locked -p corktop -- --board "path/to/folder"` opens the same board directly.

Double-click empty space to start a note. Type the title on the first line and the body below it. Press **Ctrl+Enter** or click outside the note to save. Drag a saved card and release it to save its position. Click **Close** to exit. Reopening the folder restores the cards. Each note is an ordinary Markdown file. Its logical position is stored in `.board.toml`.

Use `--identity` for build information or `--version-window` for the Pilot version screen.
