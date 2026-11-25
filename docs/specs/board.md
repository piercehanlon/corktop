# Pilot board

Pilot serves a desktop user who keeps short notes in one folder and arranges related notes together. Desktop-layer integration, tray controls, folder polling, and visual design follow Pilot.

| Requirement | Acceptance |
|---|---|
| REQ-BOARD-001 · Installed skeleton | On Windows and Linux the installed binary shows only its version in a standard borderless window. Its reported SHA matches main. Health and readiness pass. A synthetic board initializes and upgrades forward without losing its marker. A repeat produces the same state. |
| REQ-BOARD-002 · Text cards | On an empty board, create a text card and move it. Its text is readable as Markdown outside corktop. Reopening the folder restores its text and logical position. An interrupted manifest update preserves a readable prior or new state. |
| REQ-BOARD-003 · Strings | Connect two cards in one folder. Move a card and reopen the board. The string still connects the same cards. Cutting the string preserves both cards and persists after reopening. |

For text cards, double-click empty canvas to enter a title on line one and a body below it. Ctrl+Enter or clicking outside saves the note. Dragging and releasing a saved card commits its new position.

Hold a card’s top-center tack for about 200 ms, drag to a different card, and release to tie a string. Right-click the strand and choose **Cut String** to remove it.

## Everyday text cards

Pasting plain text on the board creates one Markdown file per paste, preserving its lines. Pasting in a draft inserts text into that draft. A single card-body click copies the current file contents after the double-click interval. Double-click opens the note through its OS file association. Moving a card or connecting its tack leaves the clipboard intact.

The hit area extends 12 logical pixels from the displayed yarn curve. Hover brightens the nearest eligible strand and widens its stroke. Opening the menu keeps that strand highlighted until dismissal or cut. Yarn casts a translucent shadow three logical pixels below the curve and passes underneath each top-center pinhead.
