//! Original synthetic pattern strokes, without external artwork or fonts.
//! cargo run -p tracedraw-cli --example pattern_strokes -- /tmp/pattern-strokes
use std::{error::Error, path::PathBuf};
use tracedraw_core::{
    live::Effect, pattern_stroke::PatternOptions, Affine, BezPath, Color, Document, Fill, Point,
    Shape, ShapeId, ShapeKind, Size, Symbol,
};

fn main() -> Result<(), Box<dyn Error>> {
    let destination = std::env::args_os()
        .nth(1)
        .map(PathBuf::from)
        .ok_or("provide an output directory")?;
    std::fs::create_dir_all(&destination)?;
    let mut doc = Document::new("Pattern stroke examples", Size::new(160.0, 110.0));
    let mut triangle = BezPath::new();
    triangle.move_to((-1.5, 1.0));
    triangle.line_to((1.5, 1.0));
    triangle.line_to((0.0, -1.0));
    triangle.close_path();
    let mut motif = Shape::new(
        ShapeId(1),
        ShapeKind::Path {
            path: triangle,
            closed: true,
        },
    );
    motif.fill = Fill::Solid(Color::rgb8(20, 180, 70));
    motif.stroke = None;
    motif.name = Some("Triangle marker".into());
    doc.symbols.push(Symbol {
        name: "Triangle marker".into(),
        shape: motif.clone(),
    });
    let mut dot = Shape::new(
        ShapeId(2),
        ShapeKind::Ellipse {
            rect: tracedraw_core::Rect::new(-1.5, -1.5, 1.5, 1.5),
            arc: None,
        },
    );
    dot.fill = Fill::Solid(Color::rgb8(30, 110, 220));
    dot.stroke = None;
    let mut chevron = BezPath::new();
    chevron.move_to((-1.5, -1.0));
    chevron.line_to((0.0, 1.0));
    chevron.line_to((1.5, -1.0));
    chevron.line_to((0.0, 0.0));
    chevron.close_path();
    let mut chevron = Shape::new(
        ShapeId(3),
        ShapeKind::Path {
            path: chevron,
            closed: true,
        },
    );
    chevron.fill = Fill::Solid(Color::rgb8(220, 90, 25));
    chevron.stroke = None;
    doc.pattern_motifs = Some(vec![
        tracedraw_core::pattern_stroke::PatternMotif {
            name: "Triangle marker".into(),
            shape: motif.clone(),
            anchor: None,
        },
        tracedraw_core::pattern_stroke::PatternMotif {
            name: "Blue dot".into(),
            shape: dot,
            anchor: None,
        },
        tracedraw_core::pattern_stroke::PatternMotif {
            name: "Orange chevron".into(),
            shape: chevron,
            anchor: None,
        },
    ]);
    let mut ids = doc.ids().clone();
    for (row, spacing, offset, rotation) in [
        (85.0, 6.0, 0.0, 0.0),
        (60.0, 10.0, 0.0, 180.0),
        (35.0, 6.0, 3.0, 0.0),
    ] {
        let mut path = BezPath::new();
        path.move_to(Point::new(0.0, 0.0));
        path.line_to((30.0, 10.0));
        path.curve_to((60.0, -20.0), (90.0, -20.0), (120.0, 5.0));
        let mut guide = Shape::new(
            ids.shape(),
            ShapeKind::Path {
                path,
                closed: false,
            },
        );
        guide.name = Some(format!(
            "Spacing {spacing} mm, offset {offset} mm, rotation {rotation}"
        ));
        guide.transform = Affine::translate((20.0, row));
        guide.fill = Fill::None;
        guide.stroke = None;
        guide.effects.push(Effect::PatternStroke {
            motif: Box::new(motif.clone()),
            options: PatternOptions {
                spacing,
                size: 3.0,
                offset,
                rotation,
                ..Default::default()
            },
            original: None,
        });
        doc.pages[0].layers[0].shapes.push(guide);
    }
    doc.set_ids(ids);
    std::fs::write(destination.join("pattern-strokes.tdraw"), doc.to_json()?)?;
    std::fs::write(
        destination.join("pattern-strokes.svg"),
        tracedraw_io::svg::page_to_svg(&doc, 0),
    )?;
    std::fs::write(
        destination.join("pattern-strokes.cdr"),
        tracedraw_cdr::write::document_to_cdr(&doc),
    )?;
    let picture =
        tracedraw_render::render_page_image(&doc, doc.pages[0].id, 150.0).ok_or("render failed")?;
    picture.save_png(destination.join("pattern-strokes.png"))?;
    Ok(())
}
