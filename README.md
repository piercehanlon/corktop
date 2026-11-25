# corktop
Lightweight, portable spatial scratchpad and pin board. Or that's what they want you to think.

## Open a folder board

Launch corktop, enter the path to an empty directory or an existing corktop board, and choose **Open folder**. From a checkout, `cargo run --locked -p corktop -- --board "path/to/folder"` opens the same board directly.

Double-click empty space to start a note. Type the title on the first line and the body below it. Press **Ctrl+Enter** or click outside the note to save. Drag a saved card and release it to save its position. Click **Close** to exit. Reopening the folder restores the cards. Each note is an ordinary Markdown file. Its logical position is stored in `.board.toml`.

To connect two cards, hold the top-center tack on one card for about 200 ms, drag onto the other card, and release. Right-click the string and choose **Cut String** to remove the connection.

Use `--identity` for build information or `--version-window` for the Pilot version screen.

Click empty board space and press **Ctrl+V** to pin plain text. Each paste gets its own Markdown file. A single click on a saved card copies the file contents and briefly shows **copied**. Double-click the card to open its file in your default Markdown application. Hover near a strand to highlight the one that **Cut String** will remove.

An occupied folder can stage its files in the **Inbox** or pin them in a grid. File drops default to Copy. Choose Copy, Link, Move, or Ask each time in Settings. In Ask mode, one dialog applies your choice to the dropped files. Cancel leaves them in place. In the other modes, Shift moves the files and Ctrl switches Copy to Link or Link/Move to Copy. The badge at the top of the board shows the pending action. Paste a URL, a bitmap, or files copied in the file manager to pin them on the board. Edits outside corktop refresh the card. Missing files retain their places and connections. Settings can dismiss missing cards after a day or a week. The default keeps them until you dismiss them.

Double-click a folder card to enter it. **Parent folder** returns to the previous board. Drop a file card onto a folder card to move its payload into that folder. Connections stay on their original board. Hover over a card for **Edit** and the recycle button. **Recycle bin** restores a deleted card and links to surviving cards. **Settings** controls whether editing a title renames the file and whether file drops default to links. These preferences travel with the selected root folder.

**Pin** places a bare tack. Drag it immediately to reposition it, or hold briefly and drag to pull a string. Drop on empty cork to plant the next tack. The mouse wheel changes the thread color while pulling. Close a loop of three or more bare tacks to name a zone. Drag its label to move the enclosed cards with it. Right-click the label for grid, ring, web, stack and folder-packing actions.

**Ctrl+Z** undoes an in-session layout edit. Resizing scales the board until it reaches its readable minimum, then scrolling pans to the edges. Right-click empty cork and choose **Refit to Current Frame** to save new bounds.

Hold a card body, right-click it, or choose **Edit** to change its paper color, show or hide its tack, reposition it, edit its title, or open its folder. The corner controls stay visible by default. Settings can auto-hide them. Drag **Move** beside the board name to move the standard window, or drag its striped lower-right grip to resize. The upper-right list can search child boards and reveal a matching card. On first launch, browse for a board folder or paste its path.
