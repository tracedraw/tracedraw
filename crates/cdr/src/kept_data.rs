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
//! drawing read. Stable identities are kept too: live fits name their
//! guide, source objects and copies by identity, rather than by local id.
//! Other readers leave the member alone; an object another
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

/// One object as the member holds it.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct KeptData {
    /// The static id it was written with.
    pub usdn: u32,
    /// Its id in the drawing saved.
    pub id: u64,
    /// Stable identity used by live fits to name their guide and members.
    /// Older members did not keep it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub uid: Option<Uid>,
    /// Its data, in order.
    pub data: Vec<(String, String)>,
    /// The object style it follows, by the style's id; older members
    /// have none.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub style: Option<u32>,
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

/// The member's bytes for the objects of `doc` written with the static
/// ids `static_ids`: those with an identity, data or an object style, and the
/// masters their clones name (with no data of their own, so a clone
/// finds its master again). None when no object has an identity, data or a style.
pub(crate) fn encode(doc: &Document, static_ids: &HashMap<ShapeId, u32>) -> Option<Vec<u8>> {
    let shapes = all_shapes(doc);
    let masters: Vec<u64> = shapes
        .iter()
        .filter_map(|s| s.data.iter().find(|(k, _)| k == CLONE_MASTER))
        .filter_map(|(_, v)| v.parse().ok())
        .collect();
    let mut objects: Vec<KeptData> = shapes
        .iter()
        .filter(|s| {
            s.uid.is_some()
                || !s.data.is_empty()
                || s.style.is_some()
                || masters.contains(&s.id.raw())
        })
        .filter_map(|s| {
            static_ids.get(&s.id).map(|&usdn| KeptData {
                usdn,
                id: s.id.raw(),
                uid: s.uid.filter(|u| u.0 != 0),
                data: s.data.clone(),
                style: s.style,
            })
        })
        .collect();
    if objects
        .iter()
        .all(|o| o.uid.is_none() && o.data.is_empty() && o.style.is_none())
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
    if kept.is_empty() {
        return 0;
    }
    // Never introduce a shared identity, even from a damaged member.
    // Identities already read from the file or restored with effects win.
    let mut used: HashSet<Uid> = all_shapes(doc).iter().filter_map(|s| s.uid).collect();
    let mut counts = HashMap::<Uid, usize>::new();
    for uid in kept.iter().filter_map(|k| k.uid) {
        *counts.entry(uid).or_default() += 1;
    }
    // The saved drawing's ids to the read drawing's.
    let renamed: HashMap<u64, ShapeId> = kept
        .iter()
        .filter_map(|k| ids.get(&k.usdn).map(|&id| (k.id, id)))
        .collect();
    let mut n = 0;
    for k in kept
        .iter()
        .filter(|k| k.uid.is_some() || !k.data.is_empty() || k.style.is_some())
    {
        let Some(&id) = ids.get(&k.usdn) else {
            continue;
        };
        let Some(s) = doc.find_shape_mut(id) else {
            continue;
        };
        if s.uid.is_none() {
            if let Some(uid) = k.uid.filter(|u| u.0 != 0 && counts.get(u) == Some(&1)) {
                if used.insert(uid) {
                    s.uid = Some(uid);
                }
            }
        }
        if s.style.is_none() {
            s.style = k.style;
        }
        for (key, value) in &k.data {
            if s.data.iter().any(|(have, _)| have == key) {
                continue;
            }
            let value = if key == CLONE_MASTER {
                match value.parse::<u64>().ok().and_then(|m| renamed.get(&m)) {
                    Some(master) => master.raw().to_string(),
                    // The master is gone: the clone stays a plain copy.
                    None => continue,
                }
            } else {
                value.clone()
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
    use tracedraw_core::{Rect, Size};

    fn drawing() -> (Document, HashMap<u32, ShapeId>) {
        let mut doc = Document::new("Identities", Size::new(100.0, 100.0));
        let mut ids = HashMap::new();
        for usdn in 1..=3 {
            let id = doc.ids_mut().shape();
            ids.insert(usdn, id);
            doc.pages[0].layers[0].shapes.push(Shape::new(
                id,
                ShapeKind::Rect {
                    rect: Rect::new(10.0, 10.0, 20.0, 20.0),
                    radius: 0.0,
                    corners: None,
                },
            ));
        }
        (doc, ids)
    }

    #[test]
    fn old_members_without_identities_still_restore_data() {
        let (mut doc, ids) = drawing();
        let kept = decode(br#"{"format":1,"objects":[{"usdn":1,"id":42,"data":[["Code","A"]]}]}"#);
        assert_eq!(kept.len(), 1);
        assert_eq!(restore(&mut doc, &kept, &ids), 1);
        let s = doc.find_shape(ids[&1]).expect("shape");
        assert_eq!(s.uid, None);
        assert_eq!(s.data, vec![("Code".into(), "A".into())]);
    }

    #[test]
    fn zero_duplicate_and_conflicting_identities_are_not_restored() {
        let (mut doc, ids) = drawing();
        doc.find_shape_mut(ids[&3]).expect("shape").uid = Some(Uid(7));
        for uids in [[0, 0, 0], [8, 8, 9], [7, 9, 10]] {
            let json = serde_json::json!({"format":1, "objects": uids.iter().enumerate().map(|(i, u)| {
                serde_json::json!({"usdn": i + 1, "id": i + 1,
                    "uid": format!("{u:032x}"), "data": [["Code", "kept"]]})
            }).collect::<Vec<_>>()});
            let mut back = doc.clone();
            restore(
                &mut back,
                &decode(&serde_json::to_vec(&json).expect("JSON")),
                &ids,
            );
            assert_eq!(back.find_shape(ids[&1]).expect("shape").uid, None);
            assert_eq!(
                back.find_shape(ids[&2]).expect("shape").uid,
                (uids[1] == 9).then_some(Uid(9))
            );
            assert_eq!(back.find_shape(ids[&3]).expect("shape").uid, Some(Uid(7)));
            assert!(back.pages[0].layers[0]
                .shapes
                .iter()
                .all(|s| !s.data.is_empty()));
        }
    }

    #[test]
    fn malformed_members_and_missing_objects_leave_the_drawing_usable() {
        let (mut doc, ids) = drawing();
        let kept = decode(br#"{"format":1,"objects":[{"usdn":99,"id":1,"uid":"a","data":[]}]}"#);
        assert_eq!(restore(&mut doc, &kept, &ids), 0);
        for bytes in [
            b"{".as_slice(),
            br#"{"format":2,"objects":[]}"#,
            br#"{"format":1,"objects":[{"usdn":1,"id":1,"uid":"invalid","data":[]}]}"#,
        ] {
            assert!(decode(bytes).is_empty());
        }
        assert!(all_shapes(&doc)
            .iter()
            .all(|s| s.uid.is_none() && s.data.is_empty()));
    }
}
