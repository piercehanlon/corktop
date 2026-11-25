# corktop
Lightweight, portable spatial scratchpad and pin board. Or that's what they want you to think.

## Open a folder board

Launch corktop, enter the path to an empty directory or an existing corktop board, and choose **Open folder**. From a checkout, `cargo run --locked -p corktop -- --board "path/to/folder"` opens the same board directly.

Double-click empty space to start a note. Type the title on the first line and the body below it. Press **Ctrl+Enter** or click outside the note to save. Drag a saved card and release it to save its position. Click **Close** to exit. Reopening the folder restores the cards. Each note is an ordinary Markdown file. Its logical position is stored in `.board.toml`.

To connect two cards, drag the top-center tack on one card directly onto the other card, and release. Right-click the string and choose **Cut String** to remove the connection.

Use `--identity` for build information or `--version-window` for the Pilot version screen.

Click empty board space and press **Ctrl+V** to pin plain text. Each paste gets its own Markdown file. A single click on a saved card copies the file contents and briefly shows **copied**. Double-click the card to open its file in your default Markdown application. Hover near a strand to highlight the one that **Cut String** will remove.

An occupied folder can stage its files in the on-board **Inbox** with a pulsing amber LED or pin them in a grid. File drops default to Copy. Choose **Ask**, **Copy**, **Link**, or **Move** under **Dropped files** during setup or in Settings. In Ask mode the board highlights during the drag. On release, the chooser lists the files. Select **Remember this choice** to use the chosen action for future drops on this board. Cancel leaves them in place. In the other modes, Shift moves the files and Ctrl switches Copy to Link or Link/Move to Copy. The badge at the top of the board shows the pending action. Paste a URL, a bitmap, or files copied in the file manager to pin them on the board. Edits outside corktop refresh the card. Missing files retain their places and connections. Settings can dismiss missing cards after a day or a week. The default keeps them until you dismiss them.

Double-click a folder card to enter it. Click the board name in the top bar to return to a parent folder. Drop a file card onto a folder card to move its payload into that folder. Connections stay on their original board. Right-click a card to edit or recycle it. **Recycle bin** restores a deleted card and links to surviving cards. **Settings** controls whether editing a title renames the file and whether file drops default to links. These preferences travel with the selected root folder.

**Place tack** places a bare tack. Drag to pull twine, or Shift-drag to reposition the tack. Drop on empty cork to plant the next tack. The mouse wheel changes the thread color while pulling. Close a loop of three or more bare tacks to name a zone. Drag its label to move the enclosed cards with it. Double-click the label to rename it. Right-click for formatting, packing, renaming or dissolving the zone. Dissolving keeps its tacks, twine and cards.

**Ctrl+Z** undoes an in-session layout edit. Widening the window exposes more usable cork. Cards stay between 198 and 242 logical pixels wide. When items extend beyond the window, click their numbered edge indicator to pan to them. Resizing preserves their saved positions. **Refit** brings them into the current frame and **Ctrl+Z** restores the layout. Right-click empty cork and choose **Refit to Current Frame** to save new bounds.

Right-click a card for its paper color, tack, title, file location and recycle actions. Right-click a strand, bare tack, zone label or empty cork for actions on that target. Drag the unused space in the top banner to move the standard window with the operating system, double-click it to maximize or restore, and drag any edge or corner to resize. Hover a rail icon for its name. Settings groups controls under **Board**, **Appearance**, and **Behavior**. **Help** lists the gestures. **Appearance** includes **Auto-hide top bar**. On first launch, browse for a board folder or paste its path.
