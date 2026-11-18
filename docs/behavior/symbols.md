# Symbols panel

The Symbols panel (Ctrl+F3) lists the drawing's symbols, each with how
many instances of it the drawing holds: on every page and master layer,
inside groups and clip frames too. An instance inside a group was not
counted, so a symbol whose copies were all grouped said 0 (#3248,
`ui::dockers2::symbol_docker_tests::grouped_symbol_instances_are_counted`).
