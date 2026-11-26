use tracedraw_core::{Command, Document, Engine, MasterScope, Rect, Shape, ShapeKind, Size};

#[test]
fn move_layer_to_page_preserves_identity_properties_and_roundtrips() {
    let mut engine = Engine::new(Document::default());
    let source = engine.document().pages[0].id;
    let layer = engine.document().pages[0].layers[0].id;
    let id = engine.new_shape_id();
    engine
        .run(&Command::AddShape {
            layer,
            shape: Shape::new(
                id,
                ShapeKind::Rect {
                    rect: Rect::new(5.0, 7.0, 20.0, 30.0),
                    radius: 0.0,
                    corners: None,
                },
            ),
        })
        .unwrap();
    for command in [
        Command::SetLayerVisible {
            layer,
            visible: false,
        },
        Command::SetLayerLocked {
            layer,
            locked: true,
        },
        Command::SetLayerPrintable {
            layer,
            printable: false,
        },
    ] {
        engine.run(&command).unwrap();
    }
    engine
        .run(&Command::AddPage {
            name: None,
            size: Size::new(70.0, 80.0),
            layer: None,
        })
        .unwrap();
    let page = engine.document().pages[1].id;
    let before = engine.document().clone();
    engine
        .run(&Command::MoveLayerToPage {
            layer,
            page,
            index: usize::MAX,
        })
        .unwrap();
    let moved = engine.document().clone();
    assert_eq!(moved.layer(layer).unwrap(), before.layer(layer).unwrap());
    assert_eq!(moved.page(source).unwrap().layers.len(), 1);
    assert_ne!(moved.page(source).unwrap().layers[0].id, layer);
    assert!(moved.page(source).unwrap().layers[0].shapes.is_empty());
    assert_eq!(moved.page(page).unwrap().layers[1].id, layer);
    assert_eq!(
        Document::from_json(&moved.to_json().unwrap()).unwrap(),
        moved
    );
    engine.undo().unwrap();
    // Undo deliberately keeps the monotonic ID allocator advanced.
    assert_eq!(engine.document().pages, before.pages);
    assert_eq!(engine.document().master, before.master);
    engine.redo().unwrap();
    assert_eq!(engine.document(), &moved);
}

#[test]
fn move_layer_to_page_validates_before_mutating_and_reorders_same_page() {
    let mut doc = Document::default();
    let page = doc.pages[0].id;
    let layer = doc.pages[0].layers[0].id;
    Command::AddLayer {
        page,
        name: "Top".into(),
    }
    .apply(&mut doc)
    .unwrap();
    let top = doc.pages[0].layers[1].id;
    Command::MoveLayerToPage {
        layer,
        page,
        index: 1,
    }
    .apply(&mut doc)
    .unwrap();
    assert_eq!(
        doc.pages[0].layers.iter().map(|l| l.id).collect::<Vec<_>>(),
        [top, layer]
    );
    Command::AddMasterLayer {
        name: "Shared".into(),
        scope: MasterScope::All,
    }
    .apply(&mut doc)
    .unwrap();
    let shared = doc.master.last().unwrap().id;
    let missing_page = doc.ids_mut().page();
    let missing_layer = doc.ids_mut().layer();
    // Identity generation itself is independent of the attempted command.
    let before = doc.clone();
    for command in [
        Command::MoveLayerToPage {
            layer,
            page: missing_page,
            index: 0,
        },
        Command::MoveLayerToPage {
            layer: missing_layer,
            page,
            index: 0,
        },
        Command::MoveLayerToPage {
            layer: shared,
            page,
            index: 0,
        },
    ] {
        assert!(command.apply(&mut doc).is_err());
        assert_eq!(doc, before);
    }
}

#[test]
fn layer_drag_fixture_has_two_pages_and_original_overlap_shapes() {
    let bytes = include_bytes!("../../../docs/examples/layer-drag/layer-drag.tdraw");
    let doc = Document::from_tdraw_bytes(bytes).unwrap();
    assert_eq!(doc.pages.len(), 2);
    assert_eq!(doc.pages[0].layers.len(), 3);
    assert_eq!(doc.pages[1].layers.len(), 1);
    assert!(doc.pages[0]
        .layers
        .iter()
        .all(|layer| layer.shapes.len() == 1));
    assert_eq!(
        doc.master
            .iter()
            .filter(|layer| layer.page_range.is_some())
            .count(),
        2
    );
}
