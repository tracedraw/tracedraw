# Dropping files on the window

Files dragged from the desktop or a file manager (in a browser: from the
computer onto the page) and let go over the TraceDraw window open or
import. The window keeps its normal appearance during the drag.

## What a drop does

Where the files are let go decides for every file let go together:

| Let go over | The file | Result |
|---|---|---|
| the drawing window of the drawing shown | File > Import reads it (vector formats, `.cdr`, `.tdraw`, `.psd`/`.psb`, pictures, installed script filters) | imported into the active layer, selected, as one undo step, its centre at the point where the pointer was released |
| the title, menus, toolbars, document tabs or dockers, with a drawing shown | File > Open reads it (`.cdr`, `.svg`, `.tdraw`...) | opened in a tab of its own, as File > Open (#1142) |
| the same | a picture or a script filter's format | a new drawing with the file imported in the middle of its page |
| anywhere, with no drawing shown (Welcome Screen or no drawing) | File > Open reads it | opened in a tab of its own, as File > Open, or its tab shown when it is open already (#645) |
| the same | a picture (`.png`, `.jpg`, `.jpeg`, `.bmp`, `.gif`, `.webp`, `.tif`, `.tiff`) or a script filter's format | a new drawing with the last used new-document settings, the file imported in the middle of its page |
| anywhere | anything else | nothing; the status bar says the file cannot be opened or imported |

The place decides the same way on every platform (#1142). A system drag
sends the window no pointer moves, so: on Windows the cursor's place is
read from the system; on Linux the patched windowing layer turns each
XDND position message (X11), or each enter and motion of the Wayland
data device, into a cursor move over the window, and on
macOS each `draggingEntered:`, `draggingUpdated:` and
`performDragOperation:`; in the browser the patched eframe turns each
`dragover` and the `drop` into a pointer move (`vendor/README.md`).
Should the place still be missing, a picture is imported into the drawing
shown, and a drawing, which could go either way (`.cdr`, `.svg`,
`.tdraw`...), is asked about in a small window in the middle: "Import into
the drawing", "Open in its own tab" or Cancel (Esc).
On a native Wayland window (#1142) the patched windowing layer takes
the drag itself, which winit 0.30 did not (no file drop reached the
window, and the file manager showed the drag as refused; `--x11` was the
way round it): a drag offering a list of files (`text/uri-list`) is
accepted as a copy and its list read at once. The windowing layer passes
the files to TraceDraw at the drop and tells the file manager the copy
is done. Over the window's frame (its title bar drawn by TraceDraw's
windowing layer) the place counts as outside the drawing window, so a
drawing opens. Only local `file:` addresses are taken.

Several files dropped on the Welcome Screen open side by side, one tab
each. The position is recorded in page coordinates when the files are
let go, so it follows the zoom and pan at the time of the drop. Imports
with no known position go in the middle of the view. A PDF imported into
the current page keeps the drop point through the import dialogs. Drops
use their own placement, independent of the File > Import option in
Options > Global > File Formats.

## Pictures dragged out of other programs (#2349)

A picture dragged out of another program's canvas or page (a paint
program, a browser) comes as the picture itself, not as a file. The
windowing layer takes it as PNG, else as a bitmap (Windows: the
registered `PNG` or `image/png` formats, then `CF_DIBV5` or `CF_DIB`;
X11 and Wayland: `image/png`, `image/bmp`, `image/jpeg`), writes it to a
file of its own in the system's temporary folder (`tracedraw-dropped-pictures`)
as the drag enters, and hands that file on like a dropped picture file:
over the drawing it goes into it, elsewhere it gets a new drawing. The
picture comes in without a name and without a link to that file
(`vendor/winit/src/picture_drop.rs`; checks
`winit_picture_drop::tests`,
`clipboard::linked_watch_tests::a_picture_dragged_in_from_another_program_has_no_link`).

## Windows drop recovery

A drag that leaves the window can leave its files listed as hovered
(#2350). A drop can also fail to reach the program, leaving the files
listed and nothing imported (#2686). Once every mouse button has been
up for half a second with the same files still listed, TraceDraw treats
them as released at the cursor. It handles the drop when the cursor is
inside the window and ignores it outside. A matching event that arrives
shortly after recovery is ignored (`drop_files::FileDrag`,
`a_file_drag_whose_drop_is_lost_ends_once_the_buttons_are_up`).
The files of a recovered drop can stay listed ahead of the next drag's
files until the window receives a drop or leave event. TraceDraw skips
those files when the next drag ends (#2940, `FileDrag::fresh`,
`a_lost_drops_files_still_listed_do_not_come_back_with_the_next`).
Minimizing the window and bringing it back does not let them go again
either. The input kept while the window was minimized appended the list
of hovered files to itself at every frame, so on restore each file was
listed many times, the copies counted as a new drag, and a file dropped
once was imported again at every restore until TraceDraw was restarted.
The kept input now takes the latest list whole (#3058, patched eframe,
`eframe_pending_input::tests`).

## Browser build

The browser hands over the bytes, not a path: they are read in the
background, kept under `/upload/<file name>` like a file picked in a
dialog, and opened or imported on a later frame.

## Checks

- `drop_files::tests` drop real `.svg`, `.png` and unknown files through
  egui's input: over the drawing window an import lands at the release
  point with zoom and pan applied, and one undo removes it. Without a
  known pointer location it lands in the middle of the view. A `.tdraw`
  dropped over the drawing is imported into it
  (`a_tdraw_dropped_on_a_drawing_is_imported_into_it`, #1142). A PDF
  imported into the current page keeps its release point through the
  import dialog. Over the menus the `.svg` opens in a new tab and the
  `.png` gets a new drawing. A drag's place reported as pointer moves
  decides between import and open
  (`the_place_of_a_drag_decides_on_every_platform`). With the pointer
  unknown the `.png` is imported and the `.svg` is imported, opened, or
  skipped as the user chooses. Without a drawing the `.svg` opens in a
  new tab and the `.png` gets a new drawing with the picture centred on
  the page. Two files on the Welcome Screen give two tabs. An unknown
  file only sets the status.
- `winit_file_drop::tests` (the Wayland part of the patched windowing
  layer, `vendor/winit/src/file_drop.rs`): file manager lists with
  escapes, comments, other hosts and schemes; hover then drop sends the
  place and the files; a drop before the list is read waits for it; a
  drag that leaves cancels the hovered files; a list read empty is read
  again at the drop. Checked by hand on a headless Wayland compositor
  (2026-11-03): a file dragged from another program's window opened on
  the Welcome Screen, was imported over the drawing window, and opened
  over the dockers. A drag taken back out left nothing behind.
