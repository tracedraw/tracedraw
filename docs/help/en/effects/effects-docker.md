# The Effects panel

An object can carry several live effects at once, applied one after the
other. The [[docker.effects]] panel lists the effects of the selected object in
the order they apply, from the top down. Open it with
[[menu.window > menu.window.dockers > menu.window.dockers_effects]].

![The Effects panel](../../img/effects-effects-docker.png)

## Managing effects {#stack}

Each row of the list has:

- a check box that turns the effect on or off. An effect turned off keeps its
  place and settings, but is not drawn, exported or printed;
- the effect's name, dimmed while it is off;
- up and down arrows that move the effect earlier or later, which changes the
  result (for example a contour of an envelope, or an envelope of a contour);
- a cross that removes the effect.

Every change is one undo step. Each effect is still edited with its own tool
or panel.

## Round corners and Slice {#outline-effects}

Two entries of the layer effects at the top of the panel rebuild the
outline as it is drawn while the object itself stays as it was, so a text
can still be edited:

- **[[effect.round_corners]]** rounds every corner of a curve, of the curves
  in a group or of the letters of a text with one radius. **Up to** limits
  it to corners sharper than an angle (180° takes every corner), and the
  **Corners** buttons take all of them, only the outer ones or only the
  inner ones.
- **[[effect.slice]]** cuts the object into parallel strips. Set how many
  strips, the angle of the cuts and the gap between them; **Shift** slides
  each strip along its cut, one way and the other with **Alternate** or by a
  random amount otherwise; **Turn** turns each strip a little, at random;
  **Seed** picks another random variation; **Round** rounds the corners of
  every strip.

[[menu.object.convert_to_curves]] (or Break Apart) makes the result plain
curves.

## Other effect commands {#commands}

- [[menu.effects > menu.effects.flatten]] turns the effects of the selected
  objects into plain objects.
- [[menu.effects.picker]] shows a list of effects to apply to the selection.
- [[menu.object > menu.object.clear_effect]] removes the effect of the
  selection.
- [[menu.object > menu.object.copy_effect]] copies an effect from another
  object, and [[menu.object.clone_effect]] makes a clone that follows it; see
  [Copying and cloning effects](copy-and-clone.md).
