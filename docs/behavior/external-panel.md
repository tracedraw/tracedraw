# External panel

Another program can show one of its own windows as a panel docked in
TraceDraw's window (Windows only, `external_panel.rs`): a tool palette of
a production program, say, that works on the open drawing through the
MCP link (`mcp.md`) and lives beside it like any other panel.

## Asking for it

Through the open window's MCP link (Options > Global > General, "Let AI
agents on this computer edit the open drawing"), the tool `dock_window`
with `hwnd`, the handle of a top-level window of the program, as a
number, and `title`, the panel's name. Refused: a handle that is no
window, a child window, a window of TraceDraw itself. A second
`dock_window` with the same window renames the panel; with another
window it replaces the first, which is handed back as below.
`undock_window` closes the panel.

## While docked

- The panel joins the right column and comes to the front like a panel
  opened from the menu; its tab and title bar show `title`. It moves,
  stacks and floats like the others.
- The window becomes a child of TraceDraw's window (`SetParent`, child
  style, caption and sizing border taken off) and covers the panel's
  page under its title bar, in pixels, every frame the page is on
  screen; it hides when the page is not (another panel in front, the
  column collapsed, the panel in a window of its own).
- The keyboard is TraceDraw's: docking gives the window no focus. A
  window that takes it (a text field clicked) keeps it until TraceDraw's
  window is clicked.
- A window closed by its program closes the panel within a second.

## Handed back

When the panel closes (its close button, `undock_window`, another
window docked) or TraceDraw closes, the window is taken out of
TraceDraw's window, gets its own style back, stays hidden and is sent
`WM_CLOSE`: its program decides what closing means. The panel is not
kept for the next session (it has no window then).

## Not done

- A panel in a window of its own shows its page empty: the window stays
  in the main window's viewport only.
- Menus and tips of TraceDraw that open over the panel's page are drawn
  under the docked window.
- The image editing mode swaps the panels out, which closes the
  external panel.
- One external panel at a time.

## Checks

Checked on screen with a test window of another process: it docks,
follows the panel's size when the column and the window are resized,
hides when another panel comes to the front, and after TraceDraw closes
its process is still running and got `WM_CLOSE`.
