# Pilot board

Pilot serves a desktop user who keeps short notes in one folder and arranges related notes together. Desktop-layer integration, tray controls, folder polling, and visual design follow Pilot.

| Requirement | Acceptance |
|---|---|
| REQ-BOARD-001 · Installed skeleton | On Windows and Linux the installed binary shows only its version in a standard borderless window. Its reported SHA matches main. Health and readiness pass. A synthetic board initializes and upgrades forward without losing its marker. A repeat produces the same state. |
| REQ-BOARD-002 · Text cards | On an empty board, create a text card and move it. Its text is readable as Markdown outside Corktop. Reopening the folder restores its text and logical position. An interrupted manifest update preserves a readable prior or new state. |
| REQ-BOARD-003 · Strings | Connect two cards in one folder. Move a card and reopen the board. The string still connects the same cards. Cutting the string preserves both cards and persists after reopening. |

| TBD | Decision point |
|---|---|
| String gestures | Confirm during design before string UI work. |

For text cards, double-click empty canvas to enter a title on line one and a body below it. Ctrl+Enter or clicking outside saves the note. Dragging and releasing a saved card commits its new position.
