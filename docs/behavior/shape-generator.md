# Shape Generator

Panel `DockerTab::ShapeGenerator` (Object > Create > Shape Generator, Window >
Panels). Code: `apps/tracedraw/src/shape_gen.rs` (geometry, pure functions of
the settings) and `ui/shape_gen_docker.rs`. Issues #3412 and #3413.

## Common rules

- Model space: mm, y up. Each generator draws into the Width x Height box
  with its bottom left at the origin (iso blocks and cone templates take
  their own size); Create centres the result on the window's middle.
- Output: one group (`ShapeKind::Group`) of `ShapeKind::Path` curves,
  added with one `Command::AddShape` (one undo step) and selected. The
  group is named after the generator and carries its settings as JSON in
  its object data under `tracedraw.shape_generator`; Update deletes it and
  adds the new group in the same batch, centred where it was.
- Paint: lines take the default outline (or Line width), solids the default
  outline's colour (black when none), cube faces greys E8, 9C and 4E with a
  hairline black outline.
- Combine merges curves of the same paint into one path (never for iso
  blocks, whose faces must stay in back-to-front order).
- At most `MAX_ITEMS` (40,000) curves; past it the result is cut and the
  status line and panel say so.
- Randomness: SplitMix64 seeded by Seed; noise is a 3D gradient noise with
  a quintic fade, two octaves (`fbm`). Same settings, same curves.
- Settings are sanitized (every value clamped, non-finite values replaced
  by defaults) before drawing and when read from preferences or a group.

## Generators

- Dot grid: pitch = longer side / Cells. Distance d from the middle,
  normalised by Falloff x half diagonal (elliptical: by the box's half
  sizes); size = pitch x Cell size x lerp(1, Edge size, smoothstep(d)).
  Lens warp inside the falloff: d' = d^(1 - 0.75 k), k = Strength / 100.
  Ripple: r' = r + k x 0.8 pitch x sin(4 x 2 pi r / falloff).
- Truchet: tiles of side Tile, a random flip each; arcs round two opposite
  corners. Band radii t/2 - 0.2t + 0.4t k/(n-1): symmetric about t/2, so
  every edge meets its neighbour whichever way it is turned, and below
  t / sqrt 2 so opposite corners' arcs never cross.
- Tessellation: tiles laid over the box's circumscribed square, turned by
  Rotation about the middle, kept when their middle is inside; Gap shrinks
  each tile towards its middle by half the gap (inradius ratio).
- Line waves: line i at y = t h; bump e(x) = exp(-(|x - w/2| / (Wave width
  w / 2))^Sharpness); y += (t - 1/2) Spread e + Amplitude e (1 - Fade |2t -
  1|) fbm(u Noise X, i Noise Y). Samples 240 to 40 by Softness; curves
  through the samples as Catmull-Rom cubics.
- Rings: radius lerp(min, max, k/(n-1)) + Amplitude fbm(cos a f, sin a f,
  k Drift), f = Detail / 2; closed Catmull-Rom curves.
- Field lines: Poles placed in the middle 70 % with charges of either sign;
  field = flow + sum q (p - c) / |p - c|^2, turned by Swirl. Streamlines
  traced both ways with a midpoint step of spacing / 4 from a shuffled
  jittered grid of seeds, stopping at the box, near a pole or nearer than
  spacing / 2 to a line already drawn (occupancy grid).
- Iso blocks: screen (x, y) = ((i - j) a cos 30, -(i + j) a sin 30 + z a);
  drawn by diagonal i + j, then upward; three faces per cube.
- Cone template: slant s = sqrt(h^2 + ((D - d) / 2)^2); radii from the apex
  r = diameter x s / |D - d|; angle = pi |D - d| / s. Equal diameters give a
  rectangle pi D by h. Pieces split the angle; the tab is a trapezoid on the
  right side, its ends chamfered by the tab width; Bleed is a second
  outline grown by the bleed. Wrap maps the selection's box: across to the
  angle (repeated Repeats times round), down to the radius from the top
  edge's to the bottom edge's; lines are cut short first so they bend.
