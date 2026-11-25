# corktop
Lightweight, portable spatial scratchpad and pin board. Or that's what they want you to think.

## Open a folder board

Launch corktop, enter the path to an empty directory or an existing corktop board, and choose **Open folder**. From a checkout, `cargo run --locked -p corktop -- --board "path/to/folder"` opens the same board directly.

Double-click empty space to start a note. Type the title on the first line and the body below it. Press **Ctrl+Enter** or click outside the note to save. Drag a saved card and release it to save its position. Click **Close** to exit. Reopening the folder restores the cards. Each note is an ordinary Markdown file. Its logical position is stored in `.board.toml`.

To connect two cards, hold the top-center tack on one card for about 200 ms, drag onto the other card, and release. Right-click the string and choose **Cut String** to remove the connection.

Use `--identity` for build information or `--version-window` for the Pilot version screen.

Click empty board space and press **Ctrl+V** to pin plain text. Each paste gets its own Markdown file. A single click on a saved card copies the file contents and briefly shows **copied**. Double-click the card to open its file in your default Markdown application. Hover near a strand to highlight the one that **Cut String** will remove.
