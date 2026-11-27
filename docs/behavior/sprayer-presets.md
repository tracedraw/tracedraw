# Sprayer presets

Expand Sprayer presets below the placement controls in Object > Fit Objects
to Path to store vector
motifs together with that panel's placement settings, including Live.
The section starts collapsed so the existing placement controls stay reachable.
The library is local to the current settings profile, shared by drawings,
and remains available after restarting. The path panel shows the current
preset and a compact preview. Browse library opens the application's
resizable dialog chrome with search, alphabetical results, thumbnail and
list views, and a larger selected-item preview. The list includes small
pictures and longer names. Search matches all typed words regardless of
case, including non-English names. The result count shows matching and
total entries. Choose a result, then Use selected preset (or double-click)
to return it to the panel. Close or Escape leaves the panel's choice
unchanged. Browsing alone does not apply anything to the drawing. Saving a preset does not change
the drawing. Applying it uses the existing placement algorithm and one
undo step, with fresh identities for the motifs and their copies.

An original [demo drawing](../examples/sprayer-presets/README.md) provides
a grouped motif, a live fit to save and a spare guide for reuse.

## Save and reuse

1. Select one vector object or group. Alternatively, select the objects
   to repeat with a guide curve last, or select any part of an existing
   live fit. A group is one repeated motif; several selected source
   objects form an ordered sequence of motifs.
2. Set spacing, copies, placement and rotation in the existing panel.
   For a repeated editable stroke, choose Fixed distance, Fill the path,
   Rotate objects to follow the path and Live.
3. Enter a name and click Save new. A duplicate name is rejected.
   To replace or rename an existing preset, choose it in the list, select
   the new source, enter the desired name and click Replace selected.
4. In this or another drawing, select one new guide curve, open Browse library, find the
   saved preset, click Use selected preset, then Apply preset to path. The saved settings are
   used. For live presets, edit the guide or the fit settings normally.

Selecting a preset alone does not modify the drawing or the panel's
placement fields. The library can rename the selected entry without a
source object selected; duplicate names are rejected. Library rename and
delete actions save immediately. Closing the library does not undo them.
The existing Shelf stores reusable objects; sprayer presets additionally
keep placement settings, so this library extends Fit Objects to Path and
uses the same picture-and-name selection pattern and bounded preview
loading without changing Shelf items. It does not replace an existing live fit: select an
unlinked guide. Delete removes the chosen library entry immediately;
existing strokes are independent copies and stay in their drawings.
There are no new shortcuts or drawing tools.

## What is saved

The source geometry, intrinsic scale and rotation, supported paints and
placement settings are saved. For a live fit the original source objects
are reconstructed by taking off their placement transforms; placed copies
and the guide are not saved. The motifs are translated together to a common centre, preserving their
relative positions for spatial ordering. Document identities, data links,
hyperlinks and style associations are not carried into a preset.
Custom centre-of-rotation pivots are not supported yet. Keep originals is
always off when reusing a preset, so no unused template is inserted.
The original motif in the source drawing is never removed by saving.

Supported motifs are rectangles, ellipses, polygons, paths and nested
groups, with no fill, solid or fountain fills (including additional
paints and masks), process colours and portable visual effects.
Text must first be converted to curves; symbol instances must be separated.
Pictures, clip frames, named inks, drawing-linked styles, pattern/texture/
mesh/procedural fills, raster effects and effects that reference other
objects are rejected with a message. No partial brush is silently saved.

The versioned library lives in the existing settings JSON. Limits are
100 presets, 32 source motifs, 128 shapes including group children,
16 nesting levels, 10,000 path elements per shape, 256 KiB per preset and
8 MiB for the library. Names have 1 to 80 characters. Invalid or future
entries are ignored without discarding other preferences. Current size
limits target small vector sprayers, not an asset catalogue.

This is a reusable motif library for Fit Objects to Path. It does not
add native linked-stroke writing for newly created or edited CDR effects.
CDR saves still retain the live fit for TraceDraw through object metadata;
other readers receive the placed vectors.

Library validation and serialization run on explicit save/apply/load,
not while drawing previews or moving the pointer. Thumbnails render only
for visible rows, with at most six new row pictures per layout pass, then
stay cached independently of
the active drawing. Replacing/deleting presets or reloading preferences
clears the cache. The browser uses one fixed thumbnail resolution so list
rows, compact selection and the larger preview share the same texture.
The cache holds at most one texture per preset, capped at 128 x 128 pixels;
failed previews are cached too. Transparent areas have a checkerboard.
The library remains keyboard focusable and provides preset names to
accessibility tools. Tests cover persistence, cross-drawing reuse,
identity isolation, grouped motifs, live updates, CDR reopen, undo/redo,
replacement/deletion, malformed entries and unsupported content.
New translations are first-pass drafts marked `# mt` for native review.
