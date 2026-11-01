//! Colour palettes: the built-in ones (generated, no licensed libraries),
//! palette files (.tdpal JSON, Adobe .ase, GIMP .gpl, Adobe .aco, XML
//! palettes) and the document palette.

use tracedraw_core::Color;

#[derive(Debug, Clone)]
pub struct Palette {
    pub name: String,
    /// i18n key when built in; empty for user palettes.
    pub key: &'static str,
    pub colors: Vec<(String, Color)>,
    pub builtin: bool,
    pub path: Option<std::path::PathBuf>,
}

impl Palette {
    pub fn name_key(&self) -> &str {
        if self.key.is_empty() {
            &self.name
        } else {
            self.key
        }
    }

    pub fn display_name(&self) -> String {
        if self.key.is_empty() {
            self.name.clone()
        } else {
            crate::i18n::tr(self.key)
        }
    }
}

/// The hue families of the default palette: a name and the full-strength
/// mix of cyan, magenta and yellow in percent, round to 10, in order round
/// the colour wheel from the pinks to the purples.
const HUE_FAMILIES: [(&str, u32, u32, u32); 12] = [
    ("Rose", 0, 100, 40),
    ("Red Orange", 0, 90, 100),
    ("Orange", 0, 60, 100),
    ("Amber", 0, 30, 100),
    ("Lime", 30, 0, 100),
    ("Leaf", 70, 0, 100),
    ("Emerald", 100, 0, 60),
    ("Teal", 100, 0, 30),
    ("Azure", 100, 40, 0),
    ("Indigo", 90, 70, 0),
    ("Violet", 60, 100, 0),
    ("Purple", 40, 100, 0),
];

/// The default CMYK palette (99 swatches), made by a rule and in this order:
///
/// 1. Black, then 90% to 10% Black, then White (11).
/// 2. The six process colours: Blue, Cyan, Green, Yellow, Red, Magenta (6).
/// 3. Twelve hue families ([`HUE_FAMILIES`], 6 each, 72): the family's mix
///    at 20%, 40%, 60% and 80% of its strength ("Orange 20%" and so on,
///    the cyan, magenta and yellow scaled, no black), the mix at full
///    strength ("Orange") and the mix with 30% black ("Dark Orange").
/// 4. Two browns, "Brown" and "Light Brown" (2).
/// 5. Eight neutrals: "Warm Grey" (M5 Y10) and "Cool Grey" (C10 M5)
///    with 20%, 40%, 60% and 80% black (8).
///
/// Every value is a whole percentage. Documents keep the colours they
/// store, so changing the palette changes nothing in a saved drawing.
pub fn default_cmyk() -> Vec<(String, Color)> {
    let mut v: Vec<(String, Color)> = Vec::with_capacity(99);
    let mut add = |n: &str, c: u32, m: u32, y: u32, k: u32| {
        v.push((
            n.to_string(),
            Color::cmyk_pct(c as f32, m as f32, y as f32, k as f32),
        ));
    };
    add("Black", 0, 0, 0, 100);
    for k in (1..=9).rev() {
        add(&format!("{}% Black", k * 10), 0, 0, 0, k * 10);
    }
    add("White", 0, 0, 0, 0);
    add("Blue", 100, 100, 0, 0);
    add("Cyan", 100, 0, 0, 0);
    add("Green", 100, 0, 100, 0);
    add("Yellow", 0, 0, 100, 0);
    add("Red", 0, 100, 100, 0);
    add("Magenta", 0, 100, 0, 0);
    for &(name, c, m, y) in &HUE_FAMILIES {
        for t in [20, 40, 60, 80] {
            add(
                &format!("{name} {t}%"),
                c * t / 100,
                m * t / 100,
                y * t / 100,
                0,
            );
        }
        add(name, c, m, y, 0);
        add(&format!("Dark {name}"), c, m, y, 30);
    }
    add("Brown", 30, 60, 80, 10);
    add("Light Brown", 10, 30, 50, 10);
    for k in [20, 40, 60, 80] {
        add(&format!("Warm Grey {k}%"), 0, 5, 10, k);
    }
    for k in [20, 40, 60, 80] {
        add(&format!("Cool Grey {k}%"), 10, 5, 0, k);
    }
    v
}

/// The default palette of an RGB drawing: the same colours as
/// [`default_cmyk`], in the same order and with the same names, as clean
/// RGB values defined directly (multiples of 17 and 10% greys), not
/// converted from the inks: each channel of a hue is the plain ink
/// formula (255 times one minus the ink plus black, to 1) rounded to the
/// nearest multiple of 17 (#620).
pub fn default_rgb() -> Vec<(String, Color)> {
    DEFAULT_RGB
        .iter()
        .map(|&(n, v)| {
            (
                n.to_string(),
                Color::rgb8((v >> 16) as u8, (v >> 8) as u8, v as u8),
            )
        })
        .collect()
}

/// [`default_rgb`]'s names and 0xRRGGBB values.
const DEFAULT_RGB: [(&str, u32); 99] = [
    ("Black", 0x000000),
    ("90% Black", 0x1A1A1A),
    ("80% Black", 0x333333),
    ("70% Black", 0x4D4D4D),
    ("60% Black", 0x666666),
    ("50% Black", 0x808080),
    ("40% Black", 0x999999),
    ("30% Black", 0xB3B3B3),
    ("20% Black", 0xCCCCCC),
    ("10% Black", 0xE6E6E6),
    ("White", 0xFFFFFF),
    ("Blue", 0x0000FF),
    ("Cyan", 0x00FFFF),
    ("Green", 0x00FF00),
    ("Yellow", 0xFFFF00),
    ("Red", 0xFF0000),
    ("Magenta", 0xFF00FF),
    ("Rose 20%", 0xFFCCEE),
    ("Rose 40%", 0xFF99DD),
    ("Rose 60%", 0xFF66BB),
    ("Rose 80%", 0xFF33AA),
    ("Rose", 0xFF0099),
    ("Dark Rose", 0xBB0055),
    ("Red Orange 20%", 0xFFCCCC),
    ("Red Orange 40%", 0xFFAA99),
    ("Red Orange 60%", 0xFF7766),
    ("Red Orange 80%", 0xFF4433),
    ("Red Orange", 0xFF1100),
    ("Dark Red Orange", 0xBB0000),
    ("Orange 20%", 0xFFDDCC),
    ("Orange 40%", 0xFFBB99),
    ("Orange 60%", 0xFFAA66),
    ("Orange 80%", 0xFF8833),
    ("Orange", 0xFF6600),
    ("Dark Orange", 0xBB2200),
    ("Amber 20%", 0xFFEECC),
    ("Amber 40%", 0xFFDD99),
    ("Amber 60%", 0xFFCC66),
    ("Amber 80%", 0xFFBB33),
    ("Amber", 0xFFBB00),
    ("Dark Amber", 0xBB6600),
    ("Lime 20%", 0xEEFFCC),
    ("Lime 40%", 0xDDFF99),
    ("Lime 60%", 0xCCFF66),
    ("Lime 80%", 0xBBFF33),
    ("Lime", 0xBBFF00),
    ("Dark Lime", 0x66BB00),
    ("Leaf 20%", 0xDDFFCC),
    ("Leaf 40%", 0xBBFF99),
    ("Leaf 60%", 0x99FF66),
    ("Leaf 80%", 0x77FF33),
    ("Leaf", 0x55FF00),
    ("Dark Leaf", 0x00BB00),
    ("Emerald 20%", 0xCCFFDD),
    ("Emerald 40%", 0x99FFBB),
    ("Emerald 60%", 0x66FFAA),
    ("Emerald 80%", 0x33FF88),
    ("Emerald", 0x00FF66),
    ("Dark Emerald", 0x00BB22),
    ("Teal 20%", 0xCCFFEE),
    ("Teal 40%", 0x99FFDD),
    ("Teal 60%", 0x66FFCC),
    ("Teal 80%", 0x33FFBB),
    ("Teal", 0x00FFBB),
    ("Dark Teal", 0x00BB66),
    ("Azure 20%", 0xCCEEFF),
    ("Azure 40%", 0x99DDFF),
    ("Azure 60%", 0x66BBFF),
    ("Azure 80%", 0x33AAFF),
    ("Azure", 0x0099FF),
    ("Dark Azure", 0x0055BB),
    ("Indigo 20%", 0xCCDDFF),
    ("Indigo 40%", 0xAABBFF),
    ("Indigo 60%", 0x7799FF),
    ("Indigo 80%", 0x4477FF),
    ("Indigo", 0x1155FF),
    ("Dark Indigo", 0x0000BB),
    ("Violet 20%", 0xDDCCFF),
    ("Violet 40%", 0xBB99FF),
    ("Violet 60%", 0xAA66FF),
    ("Violet 80%", 0x8833FF),
    ("Violet", 0x6600FF),
    ("Dark Violet", 0x2200BB),
    ("Purple 20%", 0xEECCFF),
    ("Purple 40%", 0xDD99FF),
    ("Purple 60%", 0xBB66FF),
    ("Purple 80%", 0xAA33FF),
    ("Purple", 0x9900FF),
    ("Dark Purple", 0x5500BB),
    ("Brown", 0x995511),
    ("Light Brown", 0xCC9966),
    ("Warm Grey 20%", 0xCCBBBB),
    ("Warm Grey 40%", 0x998888),
    ("Warm Grey 60%", 0x665555),
    ("Warm Grey 80%", 0x332211),
    ("Cool Grey 20%", 0xBBBBCC),
    ("Cool Grey 40%", 0x888899),
    ("Cool Grey 60%", 0x555566),
    ("Cool Grey 80%", 0x112233),
];

/// Process colours in 10% steps of C, M, Y and K combinations (a subset).
pub fn process_palette() -> Vec<(String, Color)> {
    let mut v = Vec::new();
    for &c in &[0.0f32, 20.0, 40.0, 60.0, 80.0, 100.0] {
        for &m in &[0.0f32, 20.0, 40.0, 60.0, 80.0, 100.0] {
            for &y in &[0.0f32, 40.0, 100.0] {
                v.push((
                    format!("C{c:.0} M{m:.0} Y{y:.0} K0"),
                    Color::cmyk_pct(c, m, y, 0.0),
                ));
            }
        }
    }
    v
}

pub fn gray_palette() -> Vec<(String, Color)> {
    (0..=20)
        .map(|i| {
            let p = i as f32 * 5.0;
            (format!("{p:.0}% Black"), Color::Gray { v: 1.0 - p / 100.0 })
        })
        .collect()
}

pub fn builtin_palettes() -> Vec<Palette> {
    vec![
        Palette {
            name: "Default CMYK".into(),
            key: "palette.default_cmyk",
            colors: default_cmyk(),
            builtin: true,
            path: None,
        },
        Palette {
            name: "Default RGB".into(),
            key: "palette.default_rgb",
            colors: default_rgb(),
            builtin: true,
            path: None,
        },
        Palette {
            name: "Process".into(),
            key: "palette.process",
            colors: process_palette(),
            builtin: true,
            path: None,
        },
        Palette {
            name: "Grayscale".into(),
            key: "palette.grayscale",
            colors: gray_palette(),
            builtin: true,
            path: None,
        },
    ]
}

// ----- palette files ---------------------------------------------------------

/// The extensions Open Palette accepts, in the order its filter lists them.
pub const PALETTE_EXTENSIONS: &[&str] = &["tdpal", "xml", "gpl", "ase", "aco", "json"];

/// A file filter label naming its extensions, "Palettes (*.tdpal, *.xml)":
/// some file dialogs show only the label, so the label lists them (#882).
pub fn filter_label(name: &str, extensions: &[&str]) -> String {
    let list: Vec<String> = extensions.iter().map(|e| format!("*.{e}")).collect();
    format!("{name} ({})", list.join(", "))
}

/// Load a palette from .tdpal (JSON), .xml, .gpl, .ase or .aco.
pub fn load_palette(path: &std::path::Path) -> Result<Palette, String> {
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    let name = path
        .file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| "Palette".into());
    let colors = match ext.as_str() {
        "gpl" => parse_gpl(&crate::files::read_to_string(path).map_err(|e| e.to_string())?),
        "ase" => parse_ase(&crate::files::read(path).map_err(|e| e.to_string())?),
        "aco" => parse_aco(&crate::files::read(path).map_err(|e| e.to_string())?),
        "xml" => parse_xml(&crate::files::read(path).map_err(|e| e.to_string())?)?,
        "tdpal" | "json" => {
            parse_tdpal(&crate::files::read_to_string(path).map_err(|e| e.to_string())?)?
        }
        other => return Err(format!("unsupported palette format: {other}")),
    };
    Ok(Palette {
        name,
        key: "",
        colors,
        builtin: false,
        path: Some(path.to_path_buf()),
    })
}

pub fn save_palette(p: &Palette, path: &std::path::Path) -> Result<(), String> {
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    let data = match ext.as_str() {
        "gpl" => {
            let mut s = format!("GIMP Palette\nName: {}\nColumns: 8\n#\n", p.name);
            for (n, c) in &p.colors {
                let [r, g, b] = c.to_rgb8();
                s.push_str(&format!("{r:3} {g:3} {b:3}\t{n}\n"));
            }
            s.into_bytes()
        }
        _ => {
            let entries: Vec<serde_json::Value> = p
                .colors
                .iter()
                .map(|(n, c)| serde_json::json!({"name": n, "color": c}))
                .collect();
            serde_json::to_vec_pretty(&serde_json::json!({"name": p.name, "colors": entries}))
                .map_err(|e| e.to_string())?
        }
    };
    crate::files::write(path, data).map_err(|e| e.to_string())
}

fn parse_tdpal(src: &str) -> Result<Vec<(String, Color)>, String> {
    let v: serde_json::Value = serde_json::from_str(src).map_err(|e| e.to_string())?;
    let mut out = Vec::new();
    if let Some(arr) = v.get("colors").and_then(|c| c.as_array()) {
        for e in arr {
            let name = e
                .get("name")
                .and_then(|n| n.as_str())
                .unwrap_or("")
                .to_string();
            if let Some(c) = e.get("color") {
                if let Ok(c) = serde_json::from_value::<Color>(c.clone()) {
                    out.push((name, c));
                }
            }
        }
    }
    Ok(out)
}

/// An XML palette: `<palette name=".."><colors><page>` then one
/// `<color cs="CMYK" name="Green" tints="1,0,1,0"/>` per colour, the
/// layout of the document palette `.cdr` files carry (see
/// `docs/cdr-format.md`). CMYK, RGB, grey, CMY, HSB, HLS, Lab and YIQ
/// colours are read with their names; spot colours and unknown colour
/// spaces are skipped. A file with no colour that can be read is an error,
/// so the status bar says why nothing was added (#882).
pub fn parse_xml(bytes: &[u8]) -> Result<Vec<(String, Color)>, String> {
    let text = String::from_utf8_lossy(bytes);
    let colors = tracedraw_cdr::container::parse_palette_xml(&text);
    if colors.is_empty() {
        let entries = text.matches("<color ").count();
        return Err(if entries == 0 {
            "no <color> entries in this XML palette".into()
        } else {
            format!(
                "none of its {entries} colours is in a colour space TraceDraw reads (spot colours are not read)"
            )
        });
    }
    Ok(colors)
}

/// GIMP palette: `r g b [name]` per line after the header.
pub fn parse_gpl(src: &str) -> Vec<(String, Color)> {
    let mut out = Vec::new();
    for line in src.lines() {
        let line = line.trim();
        if line.is_empty()
            || line.starts_with('#')
            || line.contains(':')
            || line.starts_with("GIMP")
        {
            continue;
        }
        let mut it = line.split_whitespace();
        let (Some(r), Some(g), Some(b)) = (it.next(), it.next(), it.next()) else {
            continue;
        };
        let (Ok(r), Ok(g), Ok(b)) = (r.parse::<u8>(), g.parse::<u8>(), b.parse::<u8>()) else {
            continue;
        };
        let name: String = it.collect::<Vec<_>>().join(" ");
        out.push((name, Color::rgb8(r, g, b)));
    }
    out
}

fn be_u16(d: &[u8], i: usize) -> Option<u16> {
    Some(u16::from_be_bytes([*d.get(i)?, *d.get(i + 1)?]))
}
fn be_u32(d: &[u8], i: usize) -> Option<u32> {
    Some(u32::from_be_bytes([
        *d.get(i)?,
        *d.get(i + 1)?,
        *d.get(i + 2)?,
        *d.get(i + 3)?,
    ]))
}
fn be_f32(d: &[u8], i: usize) -> Option<f32> {
    Some(f32::from_bits(be_u32(d, i)?))
}

/// Adobe Swatch Exchange: blocks of colour entries with UTF-16 names.
/// The ink number a palette's spot colour named `name` goes by: the same
/// name gives the same number in every drawing (FNV-1a, the top bit set
/// so it never meets the small numbers a drawing gives its own inks).
pub fn spot_ink_id(name: &str) -> u32 {
    let mut h: u32 = 0x811c_9dc5;
    for b in name.trim().bytes() {
        h ^= u32::from(b);
        h = h.wrapping_mul(0x0100_0193);
    }
    h | 0x8000_0000
}

/// A palette's spot colour: the ink named `name` at full strength, with
/// `c` in process inks as its stand-in.
pub fn spot_color(name: &str, c: Color) -> Color {
    let [cc, m, y, k] = match c.convert_to("CMYK") {
        Color::Cmyk { c, m, y, k } => [c, m, y, k],
        _ => [0.0, 0.0, 0.0, 1.0],
    };
    Color::Spot {
        ink: spot_ink_id(name),
        tint: 1.0,
        c: cc,
        m,
        y,
        k,
    }
}

pub fn parse_ase(d: &[u8]) -> Vec<(String, Color)> {
    let mut out = Vec::new();
    if d.len() < 12 || &d[0..4] != b"ASEF" {
        return out;
    }
    let count = be_u32(d, 8).unwrap_or(0) as usize;
    let mut pos = 12;
    for _ in 0..count {
        let Some(kind) = be_u16(d, pos) else { break };
        let Some(len) = be_u32(d, pos + 2) else { break };
        let body = pos + 6;
        if kind == 0x0001 {
            // Colour entry: name length (u16, chars incl. null), UTF-16BE name, model (4), values, type (u16).
            let Some(nlen) = be_u16(d, body) else { break };
            let name_bytes = (nlen as usize) * 2;
            let name_start = body + 2;
            let mut units = Vec::new();
            let mut i = name_start;
            while i + 1 < name_start + name_bytes {
                if let Some(u) = be_u16(d, i) {
                    if u != 0 {
                        units.push(u);
                    }
                }
                i += 2;
            }
            let name = String::from_utf16_lossy(&units);
            let mpos = name_start + name_bytes;
            let model = d.get(mpos..mpos + 4).unwrap_or(b"RGB ");
            let v = |k: usize| be_f32(d, mpos + 4 + 4 * k).unwrap_or(0.0);
            let values = match model {
                b"CMYK" => 4,
                b"Gray" => 1,
                _ => 3,
            };
            // After the values, the colour type: 0 global, 1 spot, 2 normal.
            let spot = be_u16(d, mpos + 4 + 4 * values) == Some(1);
            let color = match model {
                b"RGB " => Color::Rgb {
                    r: v(0),
                    g: v(1),
                    b: v(2),
                },
                b"CMYK" => Color::Cmyk {
                    c: v(0),
                    m: v(1),
                    y: v(2),
                    k: v(3),
                },
                b"LAB " => Color::Lab {
                    l: v(0) * 100.0,
                    a: v(1),
                    b: v(2),
                },
                b"Gray" => Color::Gray { v: v(0) },
                _ => Color::BLACK,
            };
            let color = if spot {
                spot_color(&name, color)
            } else {
                color
            };
            out.push((name, color));
        }
        pos = body + len as usize;
        if pos > d.len() {
            break;
        }
    }
    out
}

/// Adobe Color (.aco) version 1 and 2.
pub fn parse_aco(d: &[u8]) -> Vec<(String, Color)> {
    let mut out = Vec::new();
    let Some(version) = be_u16(d, 0) else {
        return out;
    };
    let count = be_u16(d, 2).unwrap_or(0) as usize;
    let mut pos = 4;
    for i in 0..count {
        let Some(space) = be_u16(d, pos) else { break };
        let w = |k: usize| be_u16(d, pos + 2 + 2 * k).unwrap_or(0) as f32;
        let color = match space {
            0 => Color::Rgb {
                r: w(0) / 65535.0,
                g: w(1) / 65535.0,
                b: w(2) / 65535.0,
            },
            1 => {
                let [r, g, b] = tracedraw_core::color::hsb_to_rgb(
                    w(0) / 65535.0 * 360.0,
                    w(1) / 65535.0,
                    w(2) / 65535.0,
                );
                Color::Rgb { r, g, b }
            }
            2 => Color::Cmyk {
                c: 1.0 - w(0) / 65535.0,
                m: 1.0 - w(1) / 65535.0,
                y: 1.0 - w(2) / 65535.0,
                k: 1.0 - w(3) / 65535.0,
            },
            8 => Color::Gray {
                v: 1.0 - w(0) / 10000.0,
            },
            _ => Color::BLACK,
        };
        pos += 10;
        let mut name = format!("Color {}", i + 1);
        if version == 2 {
            // u16 zero? then u32 length (chars incl. null) and UTF-16BE.
            let _ = be_u16(d, pos);
            let len = be_u32(d, pos + 2).unwrap_or(0) as usize;
            let start = pos + 6;
            let mut units = Vec::new();
            for k in 0..len.saturating_sub(1) {
                if let Some(u) = be_u16(d, start + 2 * k) {
                    units.push(u);
                }
            }
            name = String::from_utf16_lossy(&units);
            pos = start + len * 2;
        }
        out.push((name, color));
    }
    out
}

// ----- harmonies ---------------------------------------------------------------

/// Colour harmony rules (Color Styles docker > Harmony Editor).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Harmony {
    Analogous,
    Complementary,
    SplitComplementary,
    Triadic,
    Tetradic,
    Monochromatic,
}

impl Harmony {
    pub const ALL: [Harmony; 6] = [
        Harmony::Analogous,
        Harmony::Complementary,
        Harmony::SplitComplementary,
        Harmony::Triadic,
        Harmony::Tetradic,
        Harmony::Monochromatic,
    ];
    pub fn key(self) -> &'static str {
        match self {
            Harmony::Analogous => "harmony.analogous",
            Harmony::Complementary => "harmony.complementary",
            Harmony::SplitComplementary => "harmony.split_complementary",
            Harmony::Triadic => "harmony.triadic",
            Harmony::Tetradic => "harmony.tetradic",
            Harmony::Monochromatic => "harmony.monochromatic",
        }
    }
    /// Colours derived from a base colour by rotating the hue.
    pub fn colors(self, base: Color) -> Vec<Color> {
        let (h, s, b) = base.to_hsb();
        let rot = |d: f64| Color::from_hsb((h + d).rem_euclid(360.0), s, b);
        match self {
            Harmony::Analogous => vec![rot(-30.0), base, rot(30.0)],
            Harmony::Complementary => vec![base, rot(180.0)],
            Harmony::SplitComplementary => vec![base, rot(150.0), rot(210.0)],
            Harmony::Triadic => vec![base, rot(120.0), rot(240.0)],
            Harmony::Tetradic => vec![base, rot(90.0), rot(180.0), rot(270.0)],
            Harmony::Monochromatic => vec![
                Color::from_hsb(h, s, (b * 0.4).max(0.0)),
                Color::from_hsb(h, s, (b * 0.7).max(0.0)),
                base,
                Color::from_hsb(h, (s * 0.6).max(0.0), b),
                Color::from_hsb(h, (s * 0.3).max(0.0), b),
            ],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The default palette follows its rule: 99 distinct names, the generic
    /// swatches first, whole percentages, the hue families in order, and an
    /// RGB twin with the same names (the strip is the same in both modes).
    #[test]
    fn the_default_palette_follows_its_rule() {
        let cmyk = default_cmyk();
        assert_eq!(cmyk.len(), 99);
        let names: Vec<&str> = cmyk.iter().map(|(n, _)| n.as_str()).collect();
        let distinct: std::collections::HashSet<&&str> = names.iter().collect();
        assert_eq!(distinct.len(), 99, "names are unique");
        assert_eq!(names[0], "Black");
        assert_eq!(names[10], "White");
        assert_eq!(
            &names[11..17],
            &["Blue", "Cyan", "Green", "Yellow", "Red", "Magenta"]
        );
        // Six swatches per hue family, light to dark.
        assert_eq!(
            &names[17..23],
            &[
                "Rose 20%",
                "Rose 40%",
                "Rose 60%",
                "Rose 80%",
                "Rose",
                "Dark Rose"
            ]
        );
        assert_eq!(names[17 + 6 * 11 + 5], "Dark Purple");
        assert_eq!(names[89], "Brown");
        assert_eq!(names[98], "Cool Grey 80%");
        let find = |n: &str| cmyk.iter().find(|(m, _)| m == n).map(|(_, c)| *c);
        assert_eq!(find("Orange"), Some(Color::cmyk_pct(0.0, 60.0, 100.0, 0.0)));
        assert_eq!(
            find("Orange 40%"),
            Some(Color::cmyk_pct(0.0, 24.0, 40.0, 0.0))
        );
        assert_eq!(
            find("Dark Orange"),
            Some(Color::cmyk_pct(0.0, 60.0, 100.0, 30.0))
        );
        // Every ink is a whole percentage.
        for (n, c) in &cmyk {
            let Color::Cmyk { c, m, y, k } = *c else {
                panic!("{n} is not CMYK");
            };
            for v in [c, m, y, k] {
                assert!(((v * 100.0).round() - v * 100.0).abs() < 0.01, "{n}");
            }
        }
        let rgb = default_rgb();
        let rgb_names: Vec<&str> = rgb.iter().map(|(n, _)| n.as_str()).collect();
        assert_eq!(rgb_names, names);
    }

    /// Open Palette lists the extensions it accepts, XML among them,
    /// and a small XML palette comes in with its names and values (#882).
    #[test]
    fn xml_palettes_open_with_names_and_values() {
        assert!(PALETTE_EXTENSIONS.contains(&"xml"));
        assert_eq!(
            filter_label("Palettes", PALETTE_EXTENSIONS),
            "Palettes (*.tdpal, *.xml, *.gpl, *.ase, *.aco, *.json)"
        );
        let xml = br#"<?xml version="1.0" encoding="utf-8"?>
<palette guid="00000000-0000-0000-0000-000000000000" name="Test Inks">
  <colors>
    <page>
      <color cs="CMYK" name="Sky &amp; Sea" tints="0.6,0.1,0,0"/>
      <color cs="RGB" name="Orange" tints="1,0.5,0"/>
      <color cs="LAB" name="L76 a-14 b66" tints="0.764706,0.447059,0.760784"/>
      <color cs="SPOT INK 1" name="Spot" tints="1"/>
    </page>
  </colors>
</palette>"#;
        let dir = std::env::temp_dir().join(format!("td-pal-xml-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("Test Inks.XML");
        std::fs::write(&path, xml).unwrap();
        let p = load_palette(&path);
        std::fs::remove_dir_all(&dir).ok();
        let p = p.unwrap();
        assert_eq!(p.name, "Test Inks");
        let names: Vec<&str> = p.colors.iter().map(|(n, _)| n.as_str()).collect();
        assert_eq!(names, ["Sky & Sea", "Orange", "L76 a-14 b66"]);
        assert_eq!(
            p.colors[0].1,
            Color::Cmyk {
                c: 0.6,
                m: 0.1,
                y: 0.0,
                k: 0.0
            }
        );
        assert_eq!(p.colors[1].1.to_rgb8(), [255, 128, 0]);
        assert!(matches!(p.colors[2].1, Color::Lab { .. }));
    }

    /// XML that holds no colour that can be read says so instead of
    /// adding an empty palette; broken XML never panics.
    #[test]
    fn xml_palettes_without_readable_colours_are_errors() {
        assert!(parse_xml(b"").is_err());
        assert!(parse_xml(b"<palette><colors></colors></palette>").is_err());
        let e = parse_xml(br#"<color cs="SPOT" name="Ink" tints="1"/>"#).unwrap_err();
        assert!(e.contains("spot"), "{e}");
        assert!(parse_xml(&[0xff, 0xfe, b'<', 0, b'c']).is_err());
        assert!(parse_xml(b"<color cs=\"CMYK\" name=\"cut").is_err());
    }

    #[test]
    fn gpl_parses_lines() {
        let v = parse_gpl("GIMP Palette\nName: x\n#\n255 0 0\tRed\n0 0 255 Blue\n");
        assert_eq!(v.len(), 2);
        assert_eq!(v[0].0, "Red");
        assert_eq!(v[1].1.to_rgb8(), [0, 0, 255]);
    }

    #[test]
    fn ase_round_trip_of_one_rgb_colour() {
        // Hand-built ASE with one RGB entry named "A".
        let mut d = Vec::new();
        d.extend_from_slice(b"ASEF");
        d.extend_from_slice(&1u16.to_be_bytes());
        d.extend_from_slice(&0u16.to_be_bytes());
        d.extend_from_slice(&1u32.to_be_bytes());
        let mut body = Vec::new();
        body.extend_from_slice(&2u16.to_be_bytes()); // name length incl. null
        body.extend_from_slice(&(b'A' as u16).to_be_bytes());
        body.extend_from_slice(&0u16.to_be_bytes());
        body.extend_from_slice(b"RGB ");
        for v in [1.0f32, 0.5, 0.0] {
            body.extend_from_slice(&v.to_be_bytes());
        }
        body.extend_from_slice(&0u16.to_be_bytes());
        d.extend_from_slice(&1u16.to_be_bytes());
        d.extend_from_slice(&(body.len() as u32).to_be_bytes());
        d.extend_from_slice(&body);
        let v = parse_ase(&d);
        assert_eq!(v.len(), 1);
        assert_eq!(v[0].0, "A");
        assert_eq!(v[0].1.to_rgb8(), [255, 128, 0]);
    }

    /// A spot entry (colour type 1) of a colour book.
    fn ase_spot(name: &str, cmyk: [f32; 4], kind: u16) -> Vec<u8> {
        let mut d = Vec::new();
        d.extend_from_slice(b"ASEF");
        d.extend_from_slice(&1u16.to_be_bytes());
        d.extend_from_slice(&0u16.to_be_bytes());
        d.extend_from_slice(&1u32.to_be_bytes());
        let units: Vec<u16> = name.encode_utf16().collect();
        let mut body = Vec::new();
        body.extend_from_slice(&((units.len() + 1) as u16).to_be_bytes());
        for u in units {
            body.extend_from_slice(&u.to_be_bytes());
        }
        body.extend_from_slice(&0u16.to_be_bytes());
        body.extend_from_slice(b"CMYK");
        for v in cmyk {
            body.extend_from_slice(&v.to_be_bytes());
        }
        body.extend_from_slice(&kind.to_be_bytes());
        d.extend_from_slice(&1u16.to_be_bytes());
        d.extend_from_slice(&(body.len() as u32).to_be_bytes());
        d.extend_from_slice(&body);
        d
    }

    #[test]
    fn colour_book_spot_entries_are_spot_colours() {
        let v = parse_ase(&ase_spot("Gold 871", [0.0, 0.2, 0.8, 0.1], 1));
        assert_eq!(v.len(), 1);
        let Color::Spot { ink, tint, m, .. } = v[0].1 else {
            panic!("{:?}", v[0].1);
        };
        assert_eq!(ink, spot_ink_id("Gold 871"));
        assert!(ink & 0x8000_0000 != 0);
        assert_eq!(tint, 1.0);
        assert!((m - 0.2).abs() < 1e-6);
        // Global and normal entries stay process colours.
        for kind in [0, 2] {
            let v = parse_ase(&ase_spot("Gold 871", [0.0, 0.2, 0.8, 0.1], kind));
            assert!(matches!(v[0].1, Color::Cmyk { .. }));
        }
        assert_eq!(spot_ink_id(" Gold 871 "), spot_ink_id("Gold 871"));
        assert_ne!(spot_ink_id("Gold 871"), spot_ink_id("Gold 872"));
    }

    /// Opening a palette file adds that one palette in a strip of its
    /// own, the default palette staying first; the same file again adds
    /// nothing more. A locked palette keeps its colours (#1343).
    #[test]
    fn opening_a_palette_adds_one_and_a_locked_one_keeps_its_colours() {
        let mut app = crate::app::App::headless();
        let before = app.palettes.len();
        let main = app.palette.clone();
        let file = |c: Color| Palette {
            name: "Mine".into(),
            key: "",
            colors: vec![("One".into(), c)],
            builtin: false,
            path: Some(std::path::PathBuf::from("/palettes/mine.xml")),
        };
        app.add_palette(file(Color::rgb8(255, 0, 0)));
        assert_eq!(app.palettes.len(), before + 1);
        assert_eq!(app.visible_palettes, vec![0, before]);
        assert_eq!(app.palette, main, "the default palette stays first");
        assert_eq!(app.extra_palettes.len(), 1);
        // The same file again: still one, with its colours read anew.
        app.add_palette(file(Color::rgb8(0, 255, 0)));
        assert_eq!(app.palettes.len(), before + 1);
        assert_eq!(app.visible_palettes, vec![0, before]);
        assert_eq!(app.palettes[before].colors[0].1, Color::rgb8(0, 255, 0));
        // Locked, the palette keeps its colours; unlocked, it takes them.
        app.toggle_palette_lock(before);
        assert!(app.palette_locked(before));
        app.add_palette(file(Color::rgb8(0, 0, 255)));
        assert_eq!(app.palettes[before].colors[0].1, Color::rgb8(0, 255, 0));
        app.toggle_palette_lock(before);
        assert!(!app.palette_locked(before));
        app.add_palette(file(Color::rgb8(0, 0, 255)));
        assert_eq!(app.palettes[before].colors[0].1, Color::rgb8(0, 0, 255));
    }

    #[test]
    fn a_colour_book_spot_colour_brings_its_ink_along() {
        use tracedraw_core::{Fill, ShapeKind};
        let mut app = crate::app::App::headless();
        let gold = spot_color("Gold 871", Color::cmyk_pct(0.0, 20.0, 80.0, 10.0));
        app.palettes.push(Palette {
            name: "Book".into(),
            key: "",
            colors: vec![("Gold 871".into(), gold)],
            builtin: false,
            path: None,
        });
        let id = app
            .new_shape(ShapeKind::Rect {
                rect: tracedraw_core::Rect::new(0.0, 0.0, 10.0, 10.0),
                radius: 0.0,
                corners: None,
            })
            .expect("shape");
        app.select(vec![id]);
        app.apply_fill(Fill::Solid(gold));
        assert_eq!(app.doc().inks.len(), 1);
        assert_eq!(app.doc().inks[0].name, "Gold 871");
        // Applied again (as an outline too): still one ink.
        app.apply_outline_color(Some(gold));
        assert_eq!(app.doc().inks.len(), 1);
        // The ink went in with the fill: undoing the outline and the fill
        // takes it out.
        app.undo();
        app.undo();
        assert!(app.doc().inks.is_empty());
    }

    #[test]
    fn harmonies_have_expected_counts() {
        let base = Color::rgb8(200, 30, 30);
        assert_eq!(Harmony::Triadic.colors(base).len(), 3);
        assert_eq!(Harmony::Tetradic.colors(base).len(), 4);
        let comp = Harmony::Complementary.colors(base)[1];
        let (h, _, _) = comp.to_hsb();
        assert!((h - 180.0).abs() < 2.0, "{h}");
    }
}
