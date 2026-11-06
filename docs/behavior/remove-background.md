# Remove Background (on this computer)

Image > Remove Background... (`menu.bitmaps.remove_background_local`,
also on the bitmap property bar and in a bitmap's context menu) opens a dialog that makes the background of
the selected bitmaps transparent without an AI service
(`bg_remove::remove_background`). The AI command stays as Remove
Background with AI (`menu.bitmaps.remove_background`).

## Dialog

A window the user sizes: the shared Before and After preview at the left
(`before-after-preview.md`: the Preview list, zoom and pan, a
checkerboard where transparent), and at the right:

| Setting | Range | At first | Meaning |
|---|---|---|---|
| Background | a list of colours | one row, Automatic | the colours to remove, one row each (see Colour list) |
| Tolerance | 0 to 100 (Delta E), whole numbers | 14 | how far a colour may be from a background colour and still be background; the tolerance of every row that has none of its own |
| Edge softness | 0 to 20 px | 1 | feather of the edge, in pixels of the bitmap (previews scale it) |
| Contiguous | on/off | on | only background connected to the border goes (through a backdrop that drifts in colour too, see Algorithm) |
| Remove specks | on/off | on | loose kept spots under 1/2000 of the picture (at least 12 px) go |
| Clean edge colors | on/off | on | half-transparent edge pixels lose the backdrop's colour |
| Brush: Erase, Restore | off | off | a drag on the preview erases more of the picture or restores it (see Brushes) |
| Size | 1 to 500 px, whole numbers | 30 | the brush's diameter, in pixels of the bitmap; `[` and `]` change it by a tenth (at least 1 px) while a brush is on and no field has the keys |
| Hardness | 0 to 100 %, whole numbers | 80 | the part of the brush's radius painted fully; past it the cover fades (smoothstep) to nothing at the edge |
| New background | Transparent, Color, Gradient | Transparent | what the removed background becomes (see New background) |

The settings sit in the shared settings panel with the Reset, undo and
redo row under it (`before-after-preview.md`): Reset puts every setting
back, the arrows step through the settings states (a slider's drag is
one step on release, a colour picked, added or removed, a row's
tolerance, a box, a brush stroke, the new background, Reset). OK changes
every selected bitmap in one undo step (`apply_to_each_bitmap`) and keeps
the settings, the colour list and the new background too, for the rest
of the session (`App::last_bg_remove`, the brush's size and hardness in
`App::last_bg_brush`): the dialog opens with them next time
(`bitmap_dialogs::open_remove_background`). The points where colours
were picked and the brush strokes belong to one bitmap and are not kept;
the strokes go to the bitmap the preview shows, the other selected
bitmaps get the rest of the settings. The "?" opens
`bitmaps/remove-background`.

### Colour list

`BgSettings::colors`, a list of `BgPick` (colour or Automatic, own
tolerance or none, the point it was picked at). Each row shows:

- a swatch: the colour, or for Automatic a half light, half dark square
  (the colours are learnt from the border, see Algorithm);
- an eyedropper: the next plain click on a preview pane sets this row's
  colour (and its seed point), then the eyedropper goes off;
- on the first row only, Automatic: the row learns its colours from the
  border again (shown as a label while it does);
- its tolerance field (0 to 100, whole numbers): showing the dialog's
  Tolerance until it is changed, then the row's own;
- an x button: the row goes; the last row cannot go (greyed), and an
  empty list is one Automatic row.

Under the rows, "+" (Add colour) turns the eyedropper on for a new row: the
next plain click on a pane adds the colour under it. With the eyedropper
on (from any row or "+"):

| Click on a pane | Does |
|---|---|
| plain | sets the row the eyedropper was taken for, or adds a row after "+"; the eyedropper goes off |
| Shift+click | adds the colour under the pointer as a new row (skipped when listed already); the eyedropper stays on |
| Alt+click | removes the row whose colour is nearest (CIE76) to the one under the pointer; an Automatic row counts with its nearest learnt colour, and on a tie the picked row goes; the eyedropper stays on |

The colour is taken from the picture before the change (a removed pixel
can be picked too). The pan button turns the eyedropper off.

While the eyedropper is on, a click on the drawing picks too: the dialog
renews the shared picker request (`color_picker::pick_for`, the request
the colour pickers' eyedroppers use), so the modal layer lets the click
through to the canvas, which samples the colour there and hands it back;
Shift and Alt work as on a pane. A colour picked on the drawing has no
seed point (the click may be off the bitmap). With the eyedropper off no
request is left waiting.

The preview is the shared live preview (`before-after-preview.md`): the
result is cached under a hash of the settings and the copy, made again only
when they change; while a slider is dragged it is worked on a small copy,
and the shown size follows when the pointer is released.

### Brushes

Erase and Restore (#1723, #1915) sit under the boxes, each a button and
its name: a click turns that brush on (pressed), a click again puts it
down; the pan button, an eyedropper or Reset put it down too. With a
brush on, a drag on a preview pane paints a stroke and a click a dab
(`before-after-preview.md`: the middle or right button and two fingers
pan, the wheel and a pinch zoom, so the preview can be zoomed in for
detail). Each stroke (`BgStroke`: Erase or Restore, the size and hardness
it was painted with, its points as fractions of the picture, a point
added once it is a pixel of the preview's copy from the last) is a step
of the dialog's undo and redo when the pointer goes up. A second finger
coming down drops the stroke begun.

The preview keeps the automatic removal's matte (`bg_remove::matte`,
cached under the settings without the strokes and the fill, and the
copy) while strokes are painted: a frame paints the strokes over it and
composes the picture (`bg_remove::finish`), nothing else is worked out
again.

### New background

The list New background (#1915): Transparent (as before), Color (one
colour swatch, the colour picker), Gradient (From and To swatches and
Angle, 0 to 360 degrees, 0 left to right, 90 top to bottom, 90 at first).
Changing the kind keeps the colours (Color's colour is the gradient's
From). The result is opaque: each pixel is the picture over the fill
(`BgFill::at`), so the rim keeps its anti-aliasing against the new
backdrop.

## Algorithm

1. Background colours: the opaque pixels of a two pixel border, sampled
   to at most 4096, clustered in CIE Lab by k-means (farthest-point seeds,
   a new seed only when at least 10 Delta E from the others, at most six,
   ten rounds); a cluster is kept when it holds at least a fifteenth of
   the border. When fewer than a third of the border pixels are opaque the
   picture already has a transparent background and is left alone.
   Each list row gives colours with its tolerance: an Automatic row the
   learnt colours, a picked row its colour. A picked colour works even
   when the border is transparent; with only Automatic rows and nothing
   learnt the picture is left alone.
2. Each pixel's background colour is the one it lies deepest inside: the
   least d_k - t_k over the colours k (d_k its CIE76 Delta E, t_k that
   colour's tolerance); its distance d and tolerance t are that colour's
   (transparent pixels: d = 0). So a pixel is background when it is
   within the tolerance of any listed colour, and the steps below work on
   the union. With every tolerance equal this is the nearest colour.
3. Opacity: 0 up to t, a smoothstep ramp over a band of max(4, 0.6 x t)
   past it, 1 beyond.
4. Contiguous: a breadth-first fill from the border pixels, and from the
   point each picked row was picked at with the 8 pixels around it (so a
   colour picked inside the subject, a hole, goes), reaches pixels
   under tolerance + band; it passes freely through pixels within the
   tolerance and at most 2 + ceil(softness) pixels into the ramp, so it
   takes the anti-aliased rim but not the subject (t + band, t and the
   band are each pixel's own, step 2). From a background
   pixel it also walks on to a neighbour whose colour differs from it by
   at most max(3, t / 3) Delta E (t of the pixel it walks from), as long
   as that neighbour is within max(3 t, t + band) of its colour: a backdrop that drifts
   away from what the border showed (a texture, a gradient, a soft
   shadow under the subject) goes whole, pixel by small step, while the
   subject's edge, a sharp step, stops the walk (its anti-aliased rim is
   taken by the ramp as before). Pixels reached by such steps are
   background whatever their distance. Unreached pixels stay opaque.
   Off: every pixel gets the ramp's opacity.
5. Remove specks: 4-connected kept parts (opacity at least 0.5) smaller
   than the limit go.
6. Edge softness: two box blurs of the opacity with the radius in pixels.
7. Clean edge colors: for 0.05 < opacity < 1, the colour is solved from
   C = a F + (1 - a) B with B the pixel's background colour (step 2),
   clamped.
8. Brush strokes, in the order painted: a stroke covers each pixel by
   its distance d to the stroke's path (its segments, or the lone dab):
   1 out to r x hardness (r the radius, size x scale / 2, at least half a
   pixel; the full cover at most r - 0.5), then a smoothstep down to 0
   over max(1, r - inner) pixels; the cover is the most any part of the
   path gives, so a stroke crossing itself does not add up. Erase
   multiplies the opacity by 1 - cover; Restore moves it towards 1 by
   the cover.
9. Clean edge colors (step 7) solves with u = max(automatic opacity,
   painted opacity), and only where the automatic removal made the
   pixel partly transparent: a pixel the brush alone thinned keeps its
   colour, one restored fully is the picture's own.
10. The new opacity multiplies the pixel's own alpha.
11. New background: with a fill, the pixel becomes C a + F (1 - a) with
    a its opacity and F the fill there (the gradient's position t is the
    pixel's centre projected on the angle's direction, from the middle,
    over half the picture's extent along it, so a preview and the
    bitmap agree), fully opaque.

## Checks

- `bg_remove::tests::the_backdrop_goes_and_the_subject_stays`: a gradient
  backdrop goes at both ends, the subject keeps its colour, a
  background-coloured dot inside it stays, a loose speck goes.
- `bg_remove::tests::everywhere_mode_takes_background_coloured_parts_inside_too`
- `bg_remove::tests::edge_pixels_lose_the_backdrops_colour`
- `bg_remove::tests::learnt_colours_cover_a_two_tone_backdrop`
- `bg_remove::tests::picked_colours_and_tolerance`
- `bg_remove::tests::a_transparent_picture_is_left_alone`
- `bg_remove::tests::feathering_softens_only_the_edge`
- `bg_remove::tests::a_drifting_backdrop_goes_and_the_subject_stays`: a
  soft shadow 36 Delta E from the white backdrop, over a faint texture,
  goes with the backdrop; the subject keeps its edge; Everywhere mode
  goes by colour alone.
- `bg_remove::tests::the_fill_goes_through_every_listed_colour`:
  Automatic alone stops at a green patch on the backdrop; Automatic and
  green (or two picked colours) take both; the subject stays.
- `bg_remove::tests::a_colour_picked_inside_goes_from_its_seed`: a third
  colour inside the subject goes from its seed point (Contiguous) or
  everywhere (off); feathering keeps the subject.
- `bg_remove::tests::each_colour_has_its_own_tolerance`
- `bg_remove::tests::the_colour_list_adds_and_removes`
- `pick_drag_tests::remove_background_adds_and_removes_colours`: "+" and a
  click add a row, Shift+click adds and keeps the eyedropper, Alt+click
  removes the nearest, x removes a row, the wheel sets a row's own whole
  tolerance, OK is one undo step and the list comes back next time.
- `pick_drag_tests::the_remove_background_eyedropper_picks_on_the_shared_preview`
- `pick_drag_tests::remove_background_picks_a_colour_on_the_drawing`: a
  click on the drawing adds a row after "+", Shift keeps the eyedropper,
  and no picker request outlives it.
- `bg_remove::tests::the_brushes_erase_and_restore`: Erase takes the
  subject under the path, Restore brings back the backdrop with its own
  colours, the order counts.
- `bg_remove::tests::hardness_fades_the_brush_edge`
- `bg_remove::tests::the_brush_keeps_its_size_on_a_smaller_copy`
- `bg_remove::tests::the_background_is_filled_with_a_colour_or_a_gradient`
- `bg_remove::tests::odd_strokes_change_nothing`
- `pick_drag_tests::remove_background_brushes_erase_and_restore`: Erase
  and a drag paint a stroke (no pan), undo and redo step over it,
  Restore and a click make a dab, `]` makes the brush larger, OK is one
  undo step with the pixels erased and restored, the brush is kept and
  the strokes are not.
- `pick_drag_tests::remove_background_brush_leaves_zoom_and_pan`: the
  wheel zooms and a middle drag pans with a brush on; the pan button and
  the eyedropper put the brush down.
- `pick_drag_tests::remove_background_two_fingers_pan_instead_of_painting`
- `pick_drag_tests::remove_background_fills_the_backdrop`: Color and
  Gradient on OK, kept for the next time.
- `pick_drag_tests::frame_times_of_the_remove_background_dialog`
  (ignored, `--release`): idle frames and a Tolerance drag with three
  colours listed on a 2000 x 2000 bitmap.
- `ui::dialogs::tests::every_dialog_draws_and_accepts_enter` draws the
  dialog.
