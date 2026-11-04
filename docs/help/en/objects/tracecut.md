# TraceCut

[[menu.tools > menu.tools.tracecut]] opens TraceCut, the tools for cutting
plotters. It is also on the standard toolbar's Launch button. Select the
objects to work on before opening it.

The tools are listed on the left by group. On the right are the chosen
tool's settings, a preview of the selection and what the tool finds, and
the buttons:

- **Apply** shows the result in the preview without changing the drawing;
- **OK** applies it and closes the window; anything it changes is undone in
  one step;
- **Cancel** closes the window without a change.

To hide TraceCut, clear [[options.show_tracecut]] in Options, General.

## Presets {#presets}

Each tool keeps its settings under names you give. Pick a preset from the
list to load it. The gear button shows the row to **Save** the settings
under a name, **Delete** a preset, or go back to the **Defaults**.

## Find Duplicates {#find-duplicates}

Objects lying exactly on top of another one would be cut twice. Set the
**tolerance**, how far apart two outlines may be and still be the same, and
choose whether OK deletes the repeats or selects them. The lowest object
of each set stays.

## Find Crossings {#find-crossings}

Outlines that cross themselves cut badly, and objects that cross each other
usually need a weld first. TraceCut finds the crossings, shows them as red
rings in the preview, and OK selects the objects that have them. Tick
**Also between objects** to find crossings between different objects too.
