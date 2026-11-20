# Properties docker

The Properties docker shows the selected objects' settings in sections,
each with a header bar that opens and closes it: Outline, Fill,
Transparency, the kind's own section (Rectangle corners, QR code,
Character for text), FX and Summary. Closed sections are remembered.
`mixed-selection.md` says how values that differ between the selected
objects are shown; `outline-pen.md`, `gradient-fill.md`,
`vector-pattern-fill.md` and `transparency.md` have the sections'
controls.

## Layout

The selection summary groups the object kind, position, size and area at
the top. Each section has a bordered surface, an icon and a titled header
with its disclosure arrow at the right. Long translated headers wrap.
The section buttons use the theme's active, hover and focus treatment.
Editor rows share a label column. Outline choices use equal-width framed
buttons, and the start and end arrow controls each occupy their own row.
Line caps, joins and outline positions use the same pictograms as the
Outline Pen dialog. Dash choices preview their actual line pattern. Each
choice has a translated name for accessibility and detailed hover tips.
Pictograms use a square 16-unit grid and solid silhouettes. Compact
buttons keep the same drawing proportions in the docker and the dialog.
They use 20 px boxes at 100% interface size. A subdued path line through
caps and joins shows the outline's placement.
Position choices keep one outline size while their reference path moves.
Section bodies size to their content and scroll within the docker.
Surfaces, borders, fields and text use the shared theme tokens.

## The buttons at the top (#2200)

A row of buttons at the top has one button per section the selection
has, lit while its section is open.

- A click on a button shows its section alone: every other section is
  closed and only its header bar remains, so the docker's content is
  what was picked at the top and there is nothing to scroll past. The
  sections of other kinds of object are closed too (Character while a
  rectangle is selected), so the docker stays on the section picked
  when the selection changes.
- A click on the button of the only open section opens all of them
  again.
- Ctrl+click or Shift+click opens or closes the section beside the
  others, as a click on its header bar does. Before #2200 a plain click
  did this, and the docker was always the whole list of sections.
- The tip of a button has the section's name and these clicks
  (`docker.section_pick_hint`).

`ui::dockers::pick_section`; test
`a_section_button_shows_its_section_alone`.

## Groups in the Outline section (#2573)

The Outline section's settings come in titled groups, each with its
title in bold in the full text colour and a light rule to the right
edge, a little space above: Appearance (paint, Hairline, width), Caps
and corners, Arrowheads, Line style (dash patterns, Align dashes, Edit
Line Style), Position and behavior (Outside, Center, Inside, Behind
fill, Scale with object), and, with objects selected, Opacity and
printing (object opacity, other outlines, Overprint outline, Outline
Pen...). Advanced stays a header that opens. An open section's header
bar has the accent down its left edge, so sections stand apart from
the groups inside them (`group_heading`, `section_header`; test
`every_outline_option_is_in_the_properties_docker`).

## Fill and Outline (#1200)

Besides the editors described in `gradient-fill.md` and `outline-pen.md`,
the Fill section ends with Overprint fill and Edit Fill..., the Outline
section with Overprint outline and Outline Pen...; the checkboxes read
all, none or mixed over the painted objects of the selection and set them
all (`overprint_state`, `App::set_overprint`, test
`overprint_is_set_from_the_docker`).

## Summary (#1202)

Figures about the selection, read-only, in the drawing's units and ruler
coordinates:

- Selection: how many objects are selected.
- Layer: the names of the layers holding them, each once (an object
  inside a group or a clip frame counts for its layer).
- Type: the kinds of the objects, each once.
- Selection dimension: width x height of the selection's bounds.
- Selection centre: the middle of those bounds.
- Rotation centre: where the centre of rotation was moved to in rotate
  mode, else the selection's middle (`App::rotation_center`).
- One line per object: kind, id and name.

Then two controls that act on the selection:

- Wrap paragraph text: None or one of the wrap styles (Contour or Square,
  text on the left, on the right, on both sides, above and below); picking
  a style turns wrapping on; the text wrap offset below is enabled while a
  style is on. The same settings as the property bar's Wrap Paragraph
  Text button and the context menu (`text-wrap.md`).
- Lock Object: locks the selected objects (Object > Lock Object); they
  leave the selection, and Object > Unlock All Objects frees them.

## Checks

- Given a selected 50 x 30 mm rectangle at (10, 10), then the Summary
  reports 1 object, 1 layer, its kind, 50 x 30 and the centre (35, 25)
  in ruler coordinates, with the rotation centre at the same point; with
  an ellipse added to the selection, 2 objects, 1 layer and 2 kinds; with
  nothing selected, no figures (test
  `the_summary_section_describes_the_selection`).
