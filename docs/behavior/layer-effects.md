# Layer effects

Effects > Layer Effects opens the Effects docker, whose top section,
Layer effects (#1141), stacks looks on the selected objects and sets each
one with its values drawn live on the page: there is no OK or Cancel,
every change is one undo step ("Layer Effects") and goes to every
selected object; the values shown are the first selected object's. Code:
`ui/layer_effects.rs`; the effects in `tracedraw_core::live::Effect`, the
pixels in `tracedraw_render` (`draw_outer_shadows`, `draw_inner_shadow`,
`draw_blurred`).

The list, one row each:

| Entry | Effect | Values |
|---|---|---|
| Bevel / Emboss | `Bevel` (Soft edge or Emboss) | type, depth, level, light angle, intensity, light and shadow colors |
| Outline | `Contour`, one step outside | width, color |
| 3D | `Extrude`, back parallel | distance and direction of the depth, light angle and intensity |
| Inner shadow | `InnerShadow` | distance, angle, blur, opacity, spread, color, blend mode |
| Outer shadow | `OuterShadow`: a drop shadow of the object's shape beneath it and all its effects draw; any number beside the object's own drop shadow | as Inner shadow |
| Inner glow | `Glow` inside: an inner shadow that does not move | radius, opacity, color, blend mode |
| Outer glow | `Glow` outside: a drop shadow that does not move, solid up to the edge and fading beyond it (feather direction Outside) | as Inner glow |
| Color overlay | `ColorOverlay`: the object's shape over it in one color | color, opacity |
| Gradient overlay | `GradientOverlay`: the shape over it with a linear gradient | from and to colors, angle, opacity |
| Gaussian blur | `GaussianBlur`: the object drawn on a layer of its own and blurred (three box passes) | radius |
| Halftone | `Halftone`: the object drawn with all its other effects on a layer of its own, then turned into dots (`tracedraw_render::halftone`) | dot, color mode, color, dot spacing, dot scale, softness, angle, center X and Y, cut by the object's edge |

- The check box switches the entry's effect on. Ticked the first time it
  adds the effect with the values shown; cleared, the effect is kept
  switched off at its place in the stack (as the stack's own switch
  below) and comes back with its values when ticked again.
- Clicking the name shows that entry's values under the list. Changing a
  value of an entry that is not on switches it on with that value.
- The cross takes the effect off the objects.
- Defaults: bevel as the Bevel docker's with level 4; outline 1 mm black;
  3D 2 mm down and right; inner shadow 1 mm at 315°, 50 %, blur 1 mm,
  Multiply; outer shadow 2.1 mm at 315°, 50 %, blur 1.5 mm, Multiply;
  glows pale yellow, 75 %, Screen, 1.5 mm inside and 2 mm outside;
  color overlay red at 50 %; gradient overlay black to white at 90°,
  50 %; blur 1 mm; halftone round RGB dots 1 mm apart at 45°, scale
  100 %, softness 10 %, centred, cut by the edge.

Inner shadow spread (0 % by default, older files read as 0 %) chokes
the feathered edge: the blurred coverage is divided by 1 minus the spread
(at most 99 %) and capped at full, so the shadow stays solid over that
share of the blur and fades over the rest. Drawn on the halftone's own
layer, an inner shadow keeps each pixel's coverage and darkens edge pixels
as fully as the inside, so no light rim turns into dots.

Halftone: a grid turned by the angle, with a dot centre at Center X and Y
across the object's bounds (0 % left and bottom), takes in each cell the
mean color of the drawn pixels. The dot's area is the tone times the scale
squared times the cell (exact until dots touch; a line is a band as wide as
the tone). RGB: three dots in the cell, red, green and blue, each as large
as its channel, added as light (white where all three meet), for light
dots on a dark ground. CMYK: cyan, magenta, yellow and black from the
color (black the lack of the brightest channel), on screens at 15°, 75°,
0° and 45° past the angle, each an ink dot multiplying white paper that
follows the object's edge. One color: dots of the chosen color as large as
the darkness (1 minus luma). Cut by the edge multiplies the dots by the
object's own alpha; off, the tone is weighed by the cell's alpha and the
dots stay whole (they reach half a cell past the edge, so bands and the
layer grow by a cell). Softness widens the dot edge up to a cell (at
least one pixel). The effect applies last, to everything the object and
its effects draw, and the object's other effects stay live under it.

Fill opacity, under the list, fades the object itself (`FillOpacity`,
which multiplies the object's opacity after the other effects took it as
it was): overlays, shadows and glows keep their own. At 100 % the effect
goes. An outer glow is solid under the object, so with the fill faded it
shows through.

The renderer draws outer shadows and glows, inner glows and the blur;
the vector exports (PDF, SVG) draw the overlays, as objects, and leave
the pixel effects out, as they do the inner shadow. A page's bands are
drawn with room for the widest of these to meet (`reach_walk`). Presets
keep all of them (`style_presets::portable`).

Tests: `live::tests::layer_effect_overlays_and_fill_opacity`,
`tests::layer_effects_shadow_glow_and_blur_the_object` (render),
`tests::halftone_turns_the_object_and_its_effects_into_dots` (render),
`tests::inner_shadow_spread_darkens_the_feathered_edge` (render),
`live::tests::an_inner_shadow_saved_without_spread_reads_back`,
`halftone::tests::*` (dot areas, RGB, CMYK, cut and whole dots, screen
angle and centre),
`ui::layer_effects::tests::entries_switch_set_and_remove_their_effects`.
