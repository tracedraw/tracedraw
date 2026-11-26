//! Object data that opens again (#2879).
//!
//! An object's data (the fields of Object Properties > Data, and what
//! TraceDraw keeps there itself, such as the link of a clone to its
//! master) has no record of its own in the file, nor has the object
//! style an object follows. A file of version 14 or later (a ZIP
//! package) carries them in a member of its own, [`MEMBER`],
//! by the static id (`usdn`) each object was written with, together with
//! the id the object had in the drawing saved. The reader gives each
//! object found by its static id its data back, and a clone's link,
//! which names its master by that saved id, names the master's id in the
//! drawing read. Other readers leave the member alone; an object another
//! program removed opens without it.

use std::collections::{HashMap, HashSet};

use serde::{Deserialize, Serialize};
use tracedraw_core::{
    document::{Shape, Uid},
    Document, ShapeId, ShapeKind,
};

/// The package member holding the data.
pub const MEMBER: &str = "tracedraw/data.json";

/// The most the member may inflate to when read.
pub(crate) const MAX_MEMBER: u64 = 16 << 20;

/// The layout of [`MEMBER`]; a reader skips a member of a later one.
const FORMAT: u32 = 1;

/// The data key of a clone naming its master's id.
const CLONE_MASTER: &str = "clone.master";

/// Dimension anchors name shapes by their drawing IDs. Keep the targets'
/// static IDs too, so those references can be translated when the CDR is
/// read and the drawing assigns fresh shape IDs.
const DIMENSION: &str = "dimension";

/// One object as the member holds it.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct KeptData {
    /// The static id it was written with.
    pub usdn: u32,
    /// Its id in the drawing saved.
    pub id: u64,
    /// Its data, in order.
    pub data: Vec<(String, String)>,
    /// The object style it follows, by the style's id; older members
    /// have none.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub style: Option<u32>,
    /// Its stable identity, kept for the objects another object's data
    /// names by it (#4275: the path, objects and copies of a live Fit
    /// Objects to Path); older members have none.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub uid: Option<Uid>,
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct Member {
    format: u32,
    objects: Vec<KeptData>,
}

/// Every object of `doc`, pages and master layers, inside groups and
/// clip frames too.
fn all_shapes(doc: &Document) -> Vec<&Shape> {
    fn walk<'a>(s: &'a Shape, out: &mut Vec<&'a Shape>) {
        out.push(s);
        match &s.kind {
            ShapeKind::Group { children } => {
                for c in children {
                    walk(c, out);
                }
            }
            ShapeKind::ClipFrame { frame, contents } => {
                walk(frame, out);
                for c in contents {
                    walk(c, out);
                }
            }
            _ => {}
        }
    }
    let mut out = Vec::new();
    for l in doc
        .pages
        .iter()
        .flat_map(|p| p.layers.iter())
        .chain(&doc.master)
    {
        for s in &l.shapes {
            walk(s, &mut out);
        }
    }
    out
}

/// The 32-hex-digit words of `text`: the identities it may name.
fn named_uids(text: &str, out: &mut std::collections::HashSet<u128>) {
    for word in text.split(|c: char| !c.is_ascii_hexdigit()) {
        if word.len() == 32 {
            if let Ok(v) = u128::from_str_radix(word, 16) {
                out.insert(v);
            }
        }
    }
}

fn dimension_anchor_ids(data: &[(String, String)], out: &mut HashSet<u64>) {
    let Some((_, text)) = data.iter().find(|(key, _)| key == DIMENSION) else {
        return;
    };
    let Ok(value) = serde_json::from_str::<serde_json::Value>(text) else {
        return;
    };
    if let Some(anchors) = value.get("anchors").and_then(serde_json::Value::as_array) {
        out.extend(
            anchors
                .iter()
                .filter_map(|anchor| anchor.get("shape").and_then(serde_json::Value::as_u64)),
        );
    }
}

fn remap_dimension_anchors(text: &str, renamed: &HashMap<u64, ShapeId>) -> String {
    let Ok(mut value) = serde_json::from_str::<serde_json::Value>(text) else {
        return text.to_string();
    };
    let Some(anchors) = value
        .get_mut("anchors")
        .and_then(serde_json::Value::as_array_mut)
    else {
        return text.to_string();
    };
    anchors.retain_mut(|anchor| {
        let Some(old) = anchor.get("shape").and_then(serde_json::Value::as_u64) else {
            return false;
        };
        let Some(new) = renamed.get(&old) else {
            // Older files did not preserve IDs for anchor targets. Let the
            // dimension go static at that end instead of binding it to an
            // unrelated object which happens to reuse the old number.
            return false;
        };
        anchor["shape"] = serde_json::Value::from(new.raw());
        true
    });
    serde_json::to_string(&value).unwrap_or_else(|_| text.to_string())
}

/// The member's bytes for the objects of `doc` written with the static
/// ids `static_ids`: those with data or an object style, the masters
/// their clones name (with no data of their own, so a clone finds its
/// master again), and the objects whose identity some object's data
/// names (#4275). None when no object has data or a style.
pub(crate) fn encode(doc: &Document, static_ids: &HashMap<ShapeId, u32>) -> Option<Vec<u8>> {
    let shapes = all_shapes(doc);
    let masters: Vec<u64> = shapes
        .iter()
        .filter_map(|s| s.data.iter().find(|(k, _)| k == CLONE_MASTER))
        .filter_map(|(_, v)| v.parse().ok())
        .collect();
    let mut named = std::collections::HashSet::new();
    let mut referenced_shapes = HashSet::new();
    for s in &shapes {
        for (_, v) in &s.data {
            named_uids(v, &mut named);
        }
        dimension_anchor_ids(&s.data, &mut referenced_shapes);
    }
    let is_named = |s: &Shape| s.uid.is_some_and(|u| named.contains(&u.0));
    let mut objects: Vec<KeptData> = shapes
        .iter()
        .filter(|s| {
            !s.data.is_empty()
                || s.style.is_some()
                || masters.contains(&s.id.raw())
                || referenced_shapes.contains(&s.id.raw())
                || is_named(s)
        })
        .filter_map(|s| {
            static_ids.get(&s.id).map(|&usdn| KeptData {
                usdn,
                id: s.id.raw(),
                data: s.data.clone(),
                style: s.style,
                uid: s.uid.filter(|_| is_named(s)),
            })
        })
        .collect();
    if objects
        .iter()
        .all(|o| o.data.is_empty() && o.style.is_none())
    {
        return None;
    }
    objects.sort_by_key(|o| o.usdn);
    serde_json::to_vec(&Member {
        format: FORMAT,
        objects,
    })
    .ok()
}

/// The objects of a member's bytes; none when it is damaged or of a
/// later layout (with a warning in the log).
pub(crate) fn decode(bytes: &[u8]) -> Vec<KeptData> {
    match serde_json::from_slice::<Member>(bytes) {
        Ok(m) if m.format == FORMAT => m.objects,
        Ok(m) => {
            log::warn!("{MEMBER} of layout {}; object data left out", m.format);
            Vec::new()
        }
        Err(e) => {
            log::warn!("{MEMBER} unreadable ({e}); object data left out");
            Vec::new()
        }
    }
}

/// Give the objects read by the static ids `ids` the data `kept` holds
/// for them (keys an object already has keep its value), and point each
/// clone at its master's id in `doc`. Returns how many objects took data.
pub(crate) fn restore(doc: &mut Document, kept: &[KeptData], ids: &HashMap<u32, ShapeId>) -> usize {
    // The saved drawing's ids to the read drawing's.
    let renamed: HashMap<u64, ShapeId> = kept
        .iter()
        .filter_map(|k| ids.get(&k.usdn).map(|&id| (k.id, id)))
        .collect();
    // Identities first (#4275), each given once and never one an object
    // of the drawing read already has.
    let mut taken: std::collections::HashSet<Uid> =
        all_shapes(doc).iter().filter_map(|s| s.uid).collect();
    let mut seen = std::collections::HashSet::new();
    let doubled: std::collections::HashSet<Uid> = kept
        .iter()
        .filter_map(|k| k.uid)
        .filter(|u| !seen.insert(*u))
        .collect();
    for k in kept {
        let Some(uid) = k.uid.filter(|u| u.0 != 0 && !doubled.contains(u)) else {
            continue;
        };
        let Some(&id) = ids.get(&k.usdn) else {
            continue;
        };
        if taken.contains(&uid) {
            continue;
        }
        let Some(s) = doc.find_shape_mut(id) else {
            continue;
        };
        if let Some(old) = s.uid.replace(uid) {
            taken.remove(&old);
        }
        taken.insert(uid);
    }
    let mut n = 0;
    for k in kept
        .iter()
        .filter(|k| !k.data.is_empty() || k.style.is_some())
    {
        let Some(&id) = ids.get(&k.usdn) else {
            continue;
        };
        let Some(s) = doc.find_shape_mut(id) else {
            continue;
        };
        if s.style.is_none() {
            s.style = k.style;
        }
        for (key, value) in &k.data {
            if s.data.iter().any(|(have, _)| have == key) {
                continue;
            }
            let value = match key.as_str() {
                CLONE_MASTER => match value.parse::<u64>().ok().and_then(|m| renamed.get(&m)) {
                    Some(master) => master.raw().to_string(),
                    // The master is gone: the clone stays a plain copy.
                    None => continue,
                },
                DIMENSION => remap_dimension_anchors(value, &renamed),
                _ => value.clone(),
            };
            s.data.push((key.clone(), value));
        }
        // No master, no link: its state goes with it.
        if !s.data.iter().any(|(key, _)| key == CLONE_MASTER) {
            s.data.retain(|(key, _)| key != "clone.state");
        }
        n += 1;
    }
    n
}

#[cfg(test)]
mod tests {
    use super::*;
    use tracedraw_core::{
        document::ShapeKind,
        geometry::{Rect, Size},
    };

    fn source_document() -> (Document, ShapeId, ShapeId) {
        let mut doc = Document::new("anchors", Size::new(100.0, 100.0));
        let mut next = doc.ids().clone();
        let target = Shape::new(
            next.shape(),
            ShapeKind::Rect {
                rect: Rect::new(10.0, 10.0, 30.0, 30.0),
                radius: 0.0,
                corners: None,
            },
        );
        let dimension_id = next.shape();
        let mut dimension = Shape::new(dimension_id, ShapeKind::Group { children: vec![] });
        dimension.data.push((
            DIMENSION.into(),
            serde_json::json!({
                "measure": {"kind":"linear", "a":{"x":10.0,"y":10.0}, "b":{"x":30.0,"y":30.0}, "at":{"x":20.0,"y":20.0}},
                "anchors": [{"point":0,"shape":target.id.raw(),"node":0}]
            })
            .to_string(),
        ));
        doc.pages[0].layers[0].shapes = vec![target, dimension];
        doc.set_ids(next);
        let old_target = doc.pages[0].layers[0].shapes[0].id;
        (doc, old_target, dimension_id)
    }

    #[test]
    fn dimension_anchor_targets_are_preserved_and_remapped() {
        let (source, old_target, old_dimension) = source_document();
        let static_ids = HashMap::from([(old_target, 11), (old_dimension, 22)]);
        let kept = decode(&encode(&source, &static_ids).expect("kept data"));
        assert!(kept.iter().any(|object| object.id == old_target.raw()));

        let mut imported = Document::new("anchors", Size::new(100.0, 100.0));
        let mut next = imported.ids().clone();
        let _unrelated = Shape::new(
            next.shape(),
            ShapeKind::Rect {
                rect: Rect::new(0.0, 0.0, 1.0, 1.0),
                radius: 0.0,
                corners: None,
            },
        );
        let new_target = Shape::new(
            next.shape(),
            ShapeKind::Rect {
                rect: Rect::new(10.0, 10.0, 30.0, 30.0),
                radius: 0.0,
                corners: None,
            },
        );
        let new_dimension = Shape::new(next.shape(), ShapeKind::Group { children: vec![] });
        let new_target_id = new_target.id;
        let new_dimension_id = new_dimension.id;
        imported.pages[0].layers[0].shapes = vec![_unrelated, new_target, new_dimension];
        imported.set_ids(next);
        restore(
            &mut imported,
            &kept,
            &HashMap::from([(11, new_target_id), (22, new_dimension_id)]),
        );

        let value: serde_json::Value = serde_json::from_str(
            &imported
                .find_shape(new_dimension_id)
                .expect("dimension")
                .data[0]
                .1,
        )
        .expect("dimension data");
        assert_eq!(
            value["anchors"][0]["shape"].as_u64(),
            Some(new_target_id.raw())
        );
    }

    #[test]
    fn dimension_anchor_to_unmapped_legacy_id_is_released() {
        let (source, old_target, old_dimension) = source_document();
        let kept = decode(
            &encode(&source, &HashMap::from([(old_dimension, 22)])).expect("dimension data"),
        );

        let mut imported = Document::new("anchors", Size::new(100.0, 100.0));
        let mut next = imported.ids().clone();
        let target = Shape::new(
            next.shape(),
            ShapeKind::Rect {
                rect: Rect::new(10.0, 10.0, 30.0, 30.0),
                radius: 0.0,
                corners: None,
            },
        );
        let dimension = Shape::new(next.shape(), ShapeKind::Group { children: vec![] });
        let dimension_id = dimension.id;
        imported.pages[0].layers[0].shapes = vec![target, dimension];
        imported.set_ids(next);
        restore(&mut imported, &kept, &HashMap::from([(22, dimension_id)]));

        let value: serde_json::Value =
            serde_json::from_str(&imported.find_shape(dimension_id).expect("dimension").data[0].1)
                .expect("dimension data");
        assert!(value["anchors"].as_array().expect("anchors").is_empty());
        assert!(!kept.iter().any(|object| object.id == old_target.raw()));
    }
}
