//! Volume and weight of sheet parts: net area x thickness = volume,
//! x density = weight; outline length x thickness = edge area (for edge
//! banding or painting the edges).

use super::csv::{self, Imported};
use super::non_negative;

/// A row of the density list.
#[derive(Debug, Clone, PartialEq)]
pub struct Density {
    pub name: String,
    /// Grams per cubic centimetre.
    pub g_cm3: f64,
}

/// The density list a new installation starts with (g/cm3).
pub fn default_densities() -> Vec<Density> {
    [
        ("Acrylic", 1.19),
        ("PVC foam", 0.55),
        ("Aluminium", 2.70),
        ("Steel", 7.85),
        ("MDF", 0.75),
        ("Plywood", 0.6),
        ("Polycarbonate", 1.20),
    ]
    .into_iter()
    .map(|(name, g_cm3)| Density {
        name: name.into(),
        g_cm3,
    })
    .collect()
}

/// Volume, weight and edge area of parts cut from a sheet.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct VolumeWeight {
    /// Net area (mm2).
    pub area: f64,
    /// Outline length (mm).
    pub length: f64,
    /// Sheet thickness (mm).
    pub thickness: f64,
    /// Density used (g/cm3).
    pub density: f64,
    /// area x thickness (mm3).
    pub volume: f64,
    /// volume / 1000 x density (g).
    pub weight: f64,
    /// length x thickness (mm2).
    pub edge_area: f64,
}

/// Volume and weight of `area` mm2 of sheet `thickness` mm thick at
/// `density` g/cm3, and the edge area of `length` mm of outline. Values
/// that are not numbers or below zero count as zero.
pub fn volume_weight(area: f64, length: f64, thickness: f64, density: f64) -> VolumeWeight {
    let area = non_negative(area);
    let length = non_negative(length);
    let thickness = non_negative(thickness);
    let density = non_negative(density);
    let volume = area * thickness;
    VolumeWeight {
        area,
        length,
        thickness,
        density,
        volume,
        weight: volume / 1000.0 * density,
        edge_area: length * thickness,
    }
}

/// The density list as CSV: `name,density_g_cm3` with a header row.
pub fn densities_to_csv(list: &[Density]) -> String {
    let mut rows = vec![vec!["name".to_string(), "density_g_cm3".into()]];
    for d in list {
        rows.push(vec![d.name.clone(), csv::number(d.g_cm3)]);
    }
    csv::write(&rows)
}

/// Read a density list (see [`densities_to_csv`]). A row needs a name and
/// a density above zero.
pub fn densities_from_csv(text: &str) -> Imported<Density> {
    csv::read_rows(text, |r| {
        let name = r.first()?.trim();
        if name.is_empty() {
            return None;
        }
        let g_cm3 = csv::parse_number(r.get(1)?).filter(|v| *v > 0.0)?;
        Some(Density {
            name: name.to_string(),
            g_cm3,
        })
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_default_list_has_the_documented_densities() {
        let d = default_densities();
        let get = |n: &str| d.iter().find(|x| x.name == n).map(|x| x.g_cm3);
        assert_eq!(get("Acrylic"), Some(1.19));
        assert_eq!(get("Steel"), Some(7.85));
        assert_eq!(d.len(), 7);
    }

    #[test]
    fn a_square_metre_of_3_mm_acrylic_weighs_3570_g() {
        // 1000 x 1000 mm, 3 mm thick: 3 000 000 mm3 = 3000 cm3 x 1.19.
        let v = volume_weight(1_000_000.0, 4000.0, 3.0, 1.19);
        assert!((v.volume - 3_000_000.0).abs() < 1e-6);
        assert!((v.weight - 3570.0).abs() < 1e-6);
        assert!((v.edge_area - 12_000.0).abs() < 1e-9);
    }

    #[test]
    fn bad_numbers_weigh_nothing() {
        let v = volume_weight(f64::NAN, f64::INFINITY, -3.0, 1.0);
        assert_eq!(v.volume, 0.0);
        assert_eq!(v.weight, 0.0);
        assert_eq!(v.edge_area, 0.0);
        assert_eq!(volume_weight(100.0, 0.0, 2.0, f64::NAN).weight, 0.0);
    }

    #[test]
    fn density_lists_go_through_csv_and_back() {
        let list = default_densities();
        let back = densities_from_csv(&densities_to_csv(&list));
        assert_eq!(back.items, list);
        let got = densities_from_csv("Foam;0,03\nBad;0\nGlass;2,5\n");
        assert_eq!(got.items.len(), 2);
        assert_eq!(got.items[0].g_cm3, 0.03);
        assert_eq!(got.skipped, 1);
    }
}
