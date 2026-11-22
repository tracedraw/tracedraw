# Symbols panel

The Symbols panel (Ctrl+F3) lists the drawing's symbols, each with how
many instances of it the drawing holds: on every page and master layer,
inside groups and clip frames too. An instance inside a group was not
counted, so a symbol whose copies were all grouped said 0 (#3248,
`ui::dockers2::symbol_docker_tests::grouped_symbol_instances_are_counted`).

Each row shows a small picture of the symbol's objects (#3249). The
picture frames the visual bounds of what the symbol actually draws
(`Shape::visual_bounds_with`), including off-origin `.cdr` definitions,
tiny or oversized symbols, strokes inside groups, spot inks, embedded
fonts and nested symbols, instead of the 10 x 10 mm placeholder box at
the origin (#3899,
`object_thumbs::tests::symbol_thumbnails_frame_off_origin_sized_and_nested_definitions`).
Inserting a symbol places the centre of its drawn bounds at the page
centre (#3899).

Editing a symbol opens its objects on a clean sheet (visibility 0, paper
across the window and a light mark at the symbol's origin, zoomed to fit
and restored on Finish or Cancel); clicking Edit in the panel edits the
definition upright at 1:1 without placing a new instance on a page that
has none (#3900,
`ops2::tests::editing_a_symbol_from_the_panel_uses_a_clean_sheet_without_adding_an_instance`).
A selected instance, or a group holding one, was outlined by a square
instead of the objects it shows, and a click inside a group of instances
missed it; both the panel and the top-left bar carry Cancel beside
Finish while editing, and the panel carries Duplicate (#3437,
`ops2::tests::symbol_instances_outline_their_objects_cancel_and_duplicate`).
