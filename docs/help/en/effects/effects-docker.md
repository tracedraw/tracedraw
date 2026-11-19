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

Under the list, the built-in looks give the selected objects a finished
look with one click: chrome, liquid metal, gold, liquid glass (which blurs
what lies beneath it) and clear glass. Each picture shows the selected object
wearing the look; one undo removes it. Your own saved presets come after them.

## Other effect commands {#commands}

- [[menu.effects > menu.effects.flatten]] turns the effects of the selected
  objects into plain objects.
- [[menu.effects > menu.effects.merge_shapes]] groups the selected objects
  and gives the group a live [[effect.offset_path]] that joins them into one
  outline, rounded where they meet and painted like the bottom object: move
  an object of the group and the shapes flow together or apart. Set
  **Smooth** in the layer effects at the top of the panel; on any object,
  **Offset** grows the outline (or shrinks it, below zero) live.
- [[menu.effects.picker]] shows a list of effects to apply to the selection.
- [[menu.object > menu.object.clear_effect]] removes the effect of the
  selection.
- [[menu.object > menu.object.copy_effect]] copies an effect from another
  object, and [[menu.object.clone_effect]] makes a clone that follows it; see
  [Copying and cloning effects](copy-and-clone.md).
