# Pattern strokes

Brush Strokes (I) > Pattern stroke repeats a saved vector motif along a
live curve. The curve keeps its identity, nodes and transform. It is the
editable guide, and only the repeated motifs are drawn.

## Panel and library

1. Select a plain vector object, or several objects with their colours.
   Open Effects > Brush Strokes and choose Pattern stroke.
2. Enter a Motif name and click Save selection as motif. The original
   objects stay on the page. Empty names get unique numbered defaults;
   an existing name gets a free numeric suffix instead of being overwritten.
   Saved motifs are kept in this document's `.tdraw` library; the motif
   library itself is not stored in interchange exports.
3. Choose a saved motif by its colour preview or name in the scrollable
   gallery. The selected row is highlighted. Existing duplicate names are
   distinguished by their entry numbers. Only visible rows request a
   preview, and unchanged thumbnails are cached.
4. To rename the chosen motif, edit Motif name and click Rename. Delete
   removes the selected library entry. Both operations are undoable;
   deleting a motif leaves existing strokes and drawing objects intact.
5. Select a curve and Apply to curve, or draw a new stroke with I.
6. To change an existing stroke, select it, choose Pattern stroke and
   Load selected stroke. Change the settings and Apply to curve. This is
   one undo step and replaces the pattern effect without adding a second
   pattern or deleting the guide.
7. A loaded imported motif can also be saved for reuse. Library entries
   and strokes hold independent snapshots: renaming or deleting a library
   entry does not change existing strokes.

Older files that kept motifs as symbols remain readable. The first library
edit copies those definitions to a separate motif list without deleting,
renumbering or changing ordinary drawing symbols. An explicitly emptied
library stays empty after saving and reopening.

Each saved motif keeps its placement origin. A newly selected object uses
its own geometric centre, even after loading an imported stroke whose
origin is elsewhere. Choosing a preset loads that preset's origin, so
turns in the guide do not displace the copies away from the curve.

Plain paths, rectangles, ellipses, polygons and groups of these are
supported. Convert text and symbol instances to curves first, and break
other live effects apart before saving them as motifs. This first version
repeats rigid motifs; it does not stretch a motif continuously along a
curve. The existing Preset, Brush and Sprayer modes keep their behaviour.

## Paint, separation and removal

Load the selected stroke to edit its settings. Fill colour and outline colour
are available in the panel; the normal palette and Properties controls also
paint the repeated motif. Paint changes leave saved library presets alone.
Changing spacing or another parameter after painting keeps the new colours.
Imported placements retain their exact geometry when only paint changes.

Break pattern apart, Ungroup (Ctrl+U) and Break Apart (Ctrl+K) turn a selected
live pattern into ordinary editable motif objects at the same position and
stacking order. A multicolour motif remains a group of its component objects.
The retained guide is unpainted so separating the pattern does not add a line
to the picture. The generated members receive fresh object identities and
come out selected. Existing ordinary groups keep their normal behaviour.

Remove pattern removes the motif repetitions while keeping the same guide,
its nodes, transform and other live effects. A guide without an outline gets
a hairline so it remains visible and selectable. These actions can be undone.

## Parameters and placement

Defaults: size 3 mm, spacing 8 mm, start and end distances 0 mm, side
offset 0 mm, rotation 0 degrees, Follow path direction on, Reverse off.
Size is the longest side of the motif's geometric bounding box, scaled
uniformly. The panel uses the drawing's units for distances; the property
bar shows millimetres. Motif fills and outlines retain their colours.

Each subpath starts a new sequence. Its samples are at
`start + k * spacing`, up to `length - end`, including an open endpoint
when it lands exactly on a sample. Closed subpaths omit a second sample
at the seam. Spacing is centre to centre for native centred motifs.
Imported motifs may retain an offset placement origin.

Reference chooses the saved centre of rotation, Bounding box or Outline box.
A single object's moved Pick-tool rotation centre is saved with its motif;
otherwise the centre is its geometric box centre. The nine-point Origin grid
is enabled for the two box references, numbered left to right from the top
row. The chosen corner, edge midpoint or centre lies on the guide. Outline
box includes outline width, including fixed-width outlines on resized motifs.
Reference and Origin are saved with each live stroke; Load selected stroke
restores them. Change them and Apply to curve to commit without moving nodes.

Lengths and tangents come from the Bezier segments, using arc length
accuracy 0.000001 mm. A copy is placed at the sampled point plus
`offset * left_normal`. It turns by `rotation`, plus the tangent angle
when Follow path direction is on. Reverse measures from the other end,
reverses the tangent and therefore reverses the positive offset side.
The guide's transform is applied to the complete result, so scaling the
guide scales its motif size and spacing together.

Non-finite numbers, nonpositive size or spacing, negative trims and
zero-length paths draw no copies. Motifs have a bounded complexity and
depth, and a stroke draws at most 4096 copies across its subpaths. An
invalid imported source never causes an unbounded generation loop.
Evaluations use the existing live-effect cache, invalidated by a guide,
motif or setting change.

## `.cdr` compatibility

The importer recognises the **observed version 27 type-28 single-motif,
uniform-spacing record**, with a guide/group link in `lnkt`. It decodes
the checked spacing and constant path-relative rotation fields, recovers
the motif's placement origin from the vectors, and checks every placement
against the regenerated result. It keeps the guide and repeated objects
as one live stroke. Unknown flags, other versions, multiple motifs,
nonuniform size or rotation, unmatched guides and mismatched placements
stay as separate vectors, with a warning for an unrecognised type-28
record. They are not assigned guessed settings.

For a recognised import, the original vector placements are kept and
drawn unchanged until the guide, motif or settings change. Editing then
regenerates from the guide and motif. The owned reference drawing's
placements differ from regeneration by at most 0.036 mm; initial import
uses the original placements. This is limited tested compatibility, not
full coverage of every brush or pattern record.

`.tdraw` retains the live parameters and any original placements. SVG,
PDF and legacy RIFF `.cdr` exports use the evaluated vector copies.
Package `.cdr` exports also use the existing TraceDraw source metadata
member, so TraceDraw reopens the effect live. That member does not make
the effect natively editable in other readers. Native linked-pattern
writing and general embedded-motif-library import are not implemented.

## Original examples and checks

[Download the original synthetic examples](../examples/pattern-strokes.zip)
for a bundle of `.tdraw`, `.cdr`, SVG and PNG files.

Run `cargo run -p tracedraw-cli --example pattern_strokes -- /tmp/pattern-strokes`
to generate `.tdraw`, `.cdr`, `.svg` and `.png` examples. They show triangle
markers on a curve at 6 and 10 mm spacing, a 180-degree turn, and a 3 mm
side offset. The source uses original geometry and no external artwork.
Useful cases include decorative borders, repeated diagram markers and
symbols following a curved annotation line.

Focused tests live in `pattern_stroke::tests`, `pattern_record::tests`,
`parse::tests::pattern_stroke_linked_group_recovers_the_guide_and_falls_back_safely`,
`write::tests::pattern_stroke_package_round_trip_keeps_editable_source`, and
`media::tests`. They cover placement, persistence, malformed parameters,
linked import and fallback, apply/edit/undo/redo, and the mode's property bar.
