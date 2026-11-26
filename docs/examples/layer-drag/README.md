# Layer dragging review drawing

Original synthetic rectangles and empty layers, made for this change by
@awd-studio, licensed MIT OR Apache-2.0. No external drawing or assets.
Regenerate from the repository root:

```sh
python3 scripts/samples/layer_drag.py
```

Open `layer-drag.tdraw` and the Objects panel. On Page 1, drag the name
**Bottom - blue** onto the upper half of **Top - orange**. Blue should cover
orange; one Undo restores the original order, and Redo repeats the move.
Expand or collapse the layers and repeat. Dragging a row's eye or lock must
not move the layer; clicking those controls still changes their flags.

Ctrl-click **Bottom - blue** and **Top - orange** to select both. Drag one
onto either half of **Destination** on Page 2. Both layers move there,
retaining their relative stacking order, contents and settings. Undo brings
both back in one step. Moving every layer leaves an empty layer on Page 1.
Save and reopen to check persistence.

The Master Page and Range Pages sections each have two empty layers for
reordering. Their scope, range and In front settings stay as they were.
A drop between those sections or from master scope onto a document-page
layer is rejected; the context menus explicitly change scope.

Try a self-drop, an adjacent drop that would leave the order unchanged,
Escape while dragging, releasing away from layer rows, and filtering by
name. Invalid drops leave the drawing unchanged. Object/group dragging
retains its existing behavior and cannot consume a whole layer.
