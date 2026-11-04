# TraceCut

TraceCut holds the tools for cutting plotters: preparing cut files,
nesting and measuring. Tools > TraceCut (and Launch > TraceCut on the
standard toolbar) opens it in a window of its own, resizable (880 x 600
at first, 660 x 460 at least). Like the other dialogs it keeps the rest
of the application waiting until it closes, and it works on the objects
selected when it opened.

Options > Global > General, **Show TraceCut in Launch and the Tools
menu** (on at first), hides both entries for those who do not cut.

The geometry is the `tracedraw-tracecut` crate (pure geometry over
page-space outlines in millimetres); the window turns its results into
commands. Contributed by LaMka21 (#170).

## Layout

- **Tool list** (left, 210 wide), grouped: Cut, Nesting, Measure. A
  group with no tools yet is not shown. The tool picked last is picked
  again the next time the window opens (kept in the settings).
- **Right side**, top to bottom: the tool's name and what it does; the
  **Preset** row; the tool's settings; the **preview**; the status line;
  **Apply**, **OK** and **Cancel**.
- **Preview**: the outlines of the selected objects fitted in a white
  box, y up, in dark grey; what the tool found in red (outlines drawn
  thicker, places as rings 6 px wide).
- **Status line** (pale yellow): what was found, or "Select the objects
  to work on." with nothing selected, or "Apply shows the result here
  without changing the drawing; OK applies it." before Apply, or that
  the result is out of date.

## Apply, OK, Cancel

- **Apply** works the result out into the preview and the status line.
  It never changes the drawing.
- **OK** (and Enter) applies the result and closes the window. Anything
  that changes the drawing is one undo step. When Apply was not pressed,
  or the drawing, the selection, the tool or a setting changed since,
  OK works the result out again first.
- **Cancel** (Esc, the close button) closes without a change.
- With nothing selected Apply and OK are greyed out.

## Presets

Every tool has named presets of its settings, kept in
`tracecut_presets.json` in the settings folder (in memory only in the
browser), one list per tool. The list shows "(none)" until one is
picked; picking a preset loads it. The gear button shows a row with the
preset's name, **Save** (keeps the settings under that name, replacing a
preset of the same name, and makes it the current one), **Delete** and
**Defaults** (puts the tool's settings back). A preset saved by an
older version that lacks a setting keeps that setting's default; a
damaged file gives no presets and is logged.

## Cut tools

### Find Duplicates

Objects lying on top of another would be cut twice. Two objects are the
same when every point of either outline lies within the **tolerance**
(0.05 mm at first, 0.001 to 10 mm) of the other's and their boxes agree
within it; the outlines are flattened within a quarter of the tolerance
and divided so that points are at most the tolerance apart. Of equal
objects the lowest in the stacking order is the original.

**On OK**: **Delete them** (at first) deletes the repeats in one undo
step ("Delete Duplicates"), locked objects left alone; **Select them**
selects them and changes nothing.

### Find Crossings

An outline crossing itself cuts badly, and objects crossing each other
usually need a weld before cutting. The outlines are flattened within a
quarter of the **tolerance** (0.05 mm at first); neighbouring segments
of one subpath and segments running along each other do not count, and
crossings closer than the tolerance for the same objects are one. With
**Also between objects** (off at first) crossings between different
objects count too. OK selects the objects that have crossings.
