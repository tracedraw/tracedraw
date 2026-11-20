//! Conservative recovery of a uniform, single-motif linked pattern.
//! Layout observed in an owned version 27 drawing, 2026-11-15.
//! Unknown modes and nonuniform placements stay as their original vectors.
use std::collections::HashMap;
use tracedraw_core::{
    geometry::Shape as _,
    live::Effect,
    pattern_stroke::{PatternOptions, PatternOriginal, MAX_COPIES},
    Affine, Baseline, Point, Shape, ShapeKind,
};

fn word(d: &[u8], at: usize) -> Option<u32> {
    Some(u32::from_le_bytes(
        d.get(at..at.checked_add(4)?)?.try_into().ok()?,
    ))
}

/// The observed link table uses an offset array and 32-byte entries.
/// Entry kind 0x100 names a linked group's spnd id, then its guide's id.
pub(crate) fn links(d: &[u8]) -> HashMap<u32, u32> {
    let mut out = HashMap::new();
    let Some(count) = word(d, 4).filter(|n| *n <= MAX_COPIES as u32) else {
        return out;
    };
    let Some(table) = word(d, 8).map(|n| n as usize) else {
        return out;
    };
    if word(d, 0).map(|n| n as usize) != Some(d.len()) {
        return out;
    }
    for i in 0..count as usize {
        let Some(at) = table.checked_add(i * 4) else {
            break;
        };
        let Some(next) = at.checked_add(4) else {
            break;
        };
        let (Some(start), Some(end)) = (word(d, at), word(d, next)) else {
            break;
        };
        let Some(entry) = d.get(start as usize..end as usize) else {
            continue;
        };
        if entry.len() != 32 || word(entry, 0) != Some(0x100) {
            continue;
        }
        if let (Some(group), Some(guide)) = (word(entry, 4), word(entry, 8)) {
            if guide != group {
                out.insert(group, guide);
            }
        }
    }
    out
}

/// A narrow observed record signature, rather than guessing at unknown flags.
/// Only centre spacing and a constant path-relative rotation are decoded.
pub(crate) fn settings(d: &[u8]) -> Option<(f64, f64)> {
    if d.len() != 692 {
        return None;
    }
    for (at, value) in [
        (0, 4),
        (4, u32::MAX),
        (8, 100_000),
        (12, 0),
        (16, 2),
        (20, 2),
        (24, 0),
        (548, 32),
        (552, 1),
        (556, 2),
        (560, 0x0064_0001),
        (564, 99),
        (572, 63_500),
        (580, 0),
        (584, 0),
        (588, 1),
        (688, 1),
    ] {
        if word(d, at) != Some(value) {
            return None;
        }
    }
    if d.get(592..688)?.iter().any(|v| *v != 0) {
        return None;
    }
    let spacing = word(d, 568)? as f64 * 0.0001;
    let rotation = word(d, 576)? as i32 as f64;
    (spacing > 0.0 && rotation.abs() <= 360.0).then_some((spacing, rotation))
}

/// Recover a motif from the owned drawing's rendered vector placements.
/// Accept only copies of the same plain geometry with uniform styles, count,
/// scale, spacing and tangent-relative orientation. Preserve every original
/// vector until the guide, motif or a parameter actually changes.
pub(crate) fn recover(
    guide: &Shape,
    rendered: &[Shape],
    spacing: f64,
    rotation: f64,
) -> Option<Effect> {
    if rendered.is_empty()
        || rendered.len() > MAX_COPIES
        || !matches!(guide.kind, ShapeKind::Path { closed: false, .. })
        || guide.transform.determinant().abs() < 1e-12
        || !guide.effects.is_empty()
    {
        return None;
    }
    let baseline = Baseline::new(&guide.local_path(), 0.0, false);
    let (start, direction) = baseline.at(0.0)?;
    if !spacing.is_finite() || spacing <= 0.0 || !baseline.length().is_finite() {
        return None;
    }
    let expected = ((baseline.length() / spacing).floor() as usize).saturating_add(1);
    if expected != rendered.len() {
        return None;
    }
    let inverse = guide.transform.inverse();
    let mut placed: Vec<Shape> = rendered
        .iter()
        .cloned()
        .map(|mut s| {
            s.transform = inverse * s.transform;
            s
        })
        .collect();
    if !placed
        .iter()
        .all(tracedraw_core::pattern_stroke::valid_motif)
    {
        return None;
    }
    let first = placed
        .iter()
        .min_by(|a, b| {
            let position =
                |s: &Shape| (s.transform * s.local_path().bounding_box().center() - start).hypot2();
            position(a).total_cmp(&position(b))
        })?
        .clone();
    if placed.iter().any(|s| {
        s.kind != first.kind
            || s.fill != first.fill
            || s.stroke != first.stroke
            || s.opacity != first.opacity
            || !s.visible
    }) {
        return None;
    }
    let angle = direction.atan2() + rotation.to_radians();
    let mut motif = first.clone();
    motif.transform =
        Affine::rotate(-angle) * Affine::translate(-start.to_vec2()) * motif.transform;
    let bounds = motif.bounds();
    let options = PatternOptions {
        spacing,
        rotation,
        size: bounds.width().max(bounds.height()),
        anchor: Some(Point::ZERO),
        ..Default::default()
    };
    let generated = tracedraw_core::pattern_stroke::copies(guide, &motif, &options);
    if generated.len() != placed.len() {
        return None;
    }
    // Placements must agree within 0.05 mm, including all motif corners.
    // The owned drawing differs by at most 0.036 mm after regeneration;
    // its original placements remain exact until edited.
    placed.sort_by(|a, b| {
        let position = |s: &Shape| {
            baseline
                .nearest(s.transform * s.local_path().bounding_box().center())
                .map_or(f64::INFINITY, |(distance, _)| distance)
        };
        position(a).total_cmp(&position(b))
    });
    for (old, new) in placed.iter().zip(&generated) {
        let new = inverse * new.transform;
        let b = first.local_path().bounding_box();
        for p in [
            b.origin(),
            Point::new(b.x1, b.y0),
            Point::new(b.x0, b.y1),
            Point::new(b.x1, b.y1),
            b.center(),
        ] {
            if (old.transform * p - new * p).hypot() > 0.05 {
                return None;
            }
        }
    }
    // collect_objects reads front to back; evaluate emits drawing order.
    let mut originals: Vec<_> = rendered.iter().rev().cloned().collect();
    for s in &mut originals {
        s.transform = inverse * s.transform;
    }
    Some(Effect::PatternStroke {
        motif: Box::new(motif.clone()),
        options: options.clone(),
        original: Some(Box::new(PatternOriginal {
            path: guide.local_path(),
            options,
            motif: Box::new(motif),
            copies: originals,
        })),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn pattern_record_rejects_truncated_and_unknown_parameters() {
        let mut d = vec![0; 692];
        for cut in 0..d.len() {
            assert!(settings(&d[..cut]).is_none());
        }
        assert!(settings(&d).is_none());
        for (at, value) in [
            (0, 4u32),
            (4, u32::MAX),
            (8, 100000),
            (16, 2),
            (20, 2),
            (548, 32),
            (552, 1),
            (556, 2),
            (560, 0x00640001),
            (564, 99),
            (568, 31000),
            (572, 63500),
            (576, 180),
            (588, 1),
            (688, 1),
        ] {
            d[at..at + 4].copy_from_slice(&value.to_le_bytes());
        }
        assert_eq!(settings(&d), Some((3.1, 180.0)));
        d[592] = 1;
        assert!(settings(&d).is_none());
        assert!(links(&[]).is_empty());
        assert!(links(&[255; 52]).is_empty());
    }
}
