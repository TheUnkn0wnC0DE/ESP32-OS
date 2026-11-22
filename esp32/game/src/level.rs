//! Level-Beschreibung als einfache Textdatei (eine Zeile pro Objekt).
//!
//! ```text
//! # Kommentar
//! spawn   x y z [gier°]
//! sea     y                                   # Schokoladensee (Tod darunter)
//! cake    x y z  w d h  [pink|choco|mint|lemon]   # Box-Plattform, Oberkante y
//! cookie  x y z  r h                          # runde Keks-Plattform
//! choco   x y z  w d h                        # Schokoriegel
//! wafer   x y z  w d h  dx dy dz periode [phase]  # bewegte Waffel
//! cherry  x y z                               # Kirsche (Punkte)
//! ice     x y z                               # Eiswürfel (kühlt)
//! donut   x y z  dx dz tempo                  # Gegner, patrouilliert
//! mallow  x y z                               # Marshmallow-Trampolin
//! spinner x y z  länge tempo                  # rotierende Zuckerstange
//! checkpoint x y z                            # Eis am Stiel
//! fridge  x y z gier° [kitchen|freezer|party|bread]  # Ziel: Kühlschrank / Gefrierfach / Tortentür / Brotkasten
//! theme   kitchen|fridge|freezer|buffet       # Licht, Himmel, See, Schmelzen oder Frieren
//! block   x y z  w d h  seite oben [glatt]    # Box-Plattform in Farben (hex), glatt = rutschig
//! mblock  x y z  w d h  seite oben  dx dy dz periode [phase]   # bewegte Box
//! cyl     x y z  r h  seite oben [glatt]      # runde Plattform (Glas, Becher, Dose)
//! deko    box x y z w d h farbe | deko cyl x y z r h farbe   # ohne Kollision
//! cocoa   x y z                               # warmer Kakao (gegen Frost)
//! hotplate x y z r [periode] [phase]          # Herdplatte: glüht im Takt (heiß = schnell schmelzen)
//! crumble x y z  w d h  seite oben            # brüchiges Eis: bricht nach kurzem Stehen weg
//! cane    x y z h | lolly x y z h farbe | cupcake x y z s | cloud x y z s
//! hint    x y z r  Text bis zum Zeilenende    # Tipp-Text in der Nähe
//! route   walk x z | jump x z | air x z | wait s | waitplat i anteil toleranz   # Autopilot
//! ```

use alloc::format;
use alloc::string::{String, ToString};
use alloc::vec::Vec;
use spatz::math::{Vec2, Vec3};

/// Umgebung eines Kapitels.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Theme {
    /// Sonnige Küche: Schokoladensee, der Pudding schmilzt.
    #[default]
    Kitchen,
    /// Im Kühlschrank: kühles Licht, Milchsee, Schmelzen langsamer (Tür offen).
    Fridge,
    /// Gefrierfach: eisblau, der Pudding friert ein (Kakao wärmt).
    Freezer,
    /// Dessert-Buffet ganz oben: Partylicht, Sahnesee.
    Buffet,
}

/// Aussehen des Ziels.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum GoalStyle {
    #[default]
    Kitchen,
    Freezer,
    Party,
    Bread,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CakeStyle {
    Pink,
    Choco,
    Mint,
    Lemon,
}

/// Ein Schritt des Autopiloten.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum RouteStep {
    /// Zum Punkt laufen.
    Walk(Vec2),
    /// Sofort springen und Richtung Punkt steuern, bis gelandet
    /// (oder von einem Trampolin hochgeschleudert).
    Jump(Vec2),
    /// In der Luft Richtung Punkt steuern (ohne zu springen), bis gelandet.
    Air(Vec2),
    /// Warten (Sekunden).
    Wait(f32),
    /// Warten, bis die bewegte Plattform `idx` ihren Bewegungsanteil `s`
    /// (0 = Start, 1 = Ende) bis auf `tol` erreicht hat.
    WaitPlat { idx: usize, s: f32, tol: f32 },
}

#[derive(Clone, Debug, PartialEq)]
pub enum Def {
    Spawn { pos: Vec3, yaw: f32 },
    Sea { y: f32 },
    Cake { pos: Vec3, size: Vec3, style: CakeStyle },
    Cookie { pos: Vec3, r: f32, h: f32 },
    Choco { pos: Vec3, size: Vec3 },
    Wafer { pos: Vec3, size: Vec3, delta: Vec3, period: f32, phase: f32 },
    Cherry(Vec3),
    Ice(Vec3),
    Donut { pos: Vec3, delta: Vec2, speed: f32 },
    Mallow(Vec3),
    Spinner { pos: Vec3, length: f32, speed: f32 },
    Checkpoint(Vec3),
    Fridge { pos: Vec3, yaw: f32, style: GoalStyle },
    Theme(Theme),
    /// Box-Plattform (Oberkante bei `pos.y`), optional bewegt.
    Block { pos: Vec3, size: Vec3, side: u32, top: u32, slippery: bool, motion: Option<(Vec3, f32, f32)> },
    /// Runde Plattform (Oberkante bei `pos.y`).
    Cyl { pos: Vec3, r: f32, h: f32, side: u32, top: u32, slippery: bool },
    /// Deko-Box ohne Kollision (Unterkante bei `pos.y`).
    PropBox { pos: Vec3, size: Vec3, color: u32 },
    /// Deko-Zylinder ohne Kollision (Unterkante bei `pos.y`).
    PropCyl { pos: Vec3, r: f32, h: f32, color: u32 },
    Cocoa(Vec3),
    /// Herdplatte (Oberkante bei `pos.y`), heiß in der ersten Hälfte jeder Periode.
    HotPlate { pos: Vec3, r: f32, period: f32, phase: f32 },
    /// Brüchige Box-Plattform (Oberkante bei `pos.y`).
    Crumble { pos: Vec3, size: Vec3, side: u32, top: u32 },
    Cane { pos: Vec3, h: f32 },
    Lolly { pos: Vec3, h: f32, color: u32 },
    Cupcake { pos: Vec3, s: f32 },
    Cloud { pos: Vec3, s: f32 },
    Hint { pos: Vec3, r: f32, text: String },
    Route(RouteStep),
}

/// Ein geparstes Level.
#[derive(Clone, Debug, Default)]
pub struct LevelDef {
    pub defs: Vec<Def>,
}

/// Eingebaute Kapitel in Spielreihenfolge: (Name, Leveltext).
pub const LEVELS: [(&str, &str); 8] = [
    ("DER KÜCHENTISCH", include_str!("../levels/level1.txt")),
    ("DER HERD", include_str!("../levels/level2_herd.txt")),
    ("IM KÜHLSCHRANK", include_str!("../levels/level3_kuehlschrank.txt")),
    ("DIE KÜHLSCHRANKTÜR", include_str!("../levels/level4_kuehlschranktuer.txt")),
    ("DAS GEFRIERFACH", include_str!("../levels/level5_gefrierfach.txt")),
    ("DIE EISHÖHLE", include_str!("../levels/level6_eishoehle.txt")),
    ("AUF DEM KÜHLSCHRANK", include_str!("../levels/level7_kuehlschrankdach.txt")),
    ("DAS DESSERT-BUFFET", include_str!("../levels/level8_buffet.txt")),
];

/// Das erste Level (für Tests).
pub const BUILTIN: &str = LEVELS[0].1;

struct Args<'a> {
    parts: Vec<&'a str>,
    line: usize,
}

impl<'a> Args<'a> {
    fn f(&self, i: usize) -> Result<f32, String> {
        let s = self.parts.get(i).ok_or_else(|| format!("Zeile {}: Wert {} fehlt", self.line, i))?;
        s.parse::<f32>().map_err(|_| format!("Zeile {}: '{}' ist keine Zahl", self.line, s))
    }
    fn f_or(&self, i: usize, d: f32) -> Result<f32, String> {
        if i < self.parts.len() { self.f(i) } else { Ok(d) }
    }
    fn v3(&self, i: usize) -> Result<Vec3, String> {
        Ok(Vec3::new(self.f(i)?, self.f(i + 1)?, self.f(i + 2)?))
    }
    fn color(&self, i: usize) -> Result<u32, String> {
        let s = self.parts.get(i).ok_or_else(|| format!("Zeile {}: Farbe {} fehlt", self.line, i))?;
        u32::from_str_radix(s.trim_start_matches("0x"), 16).map_err(|_| format!("Zeile {}: ungültige Farbe '{}'", self.line, s))
    }
    fn flag(&self, word: &str) -> bool {
        self.parts.iter().skip(1).any(|p| *p == word)
    }
}

pub fn parse(text: &str) -> Result<LevelDef, String> {
    let mut defs = Vec::new();
    for (ln, raw) in text.lines().enumerate() {
        let line = raw.split('#').next().unwrap_or("").trim();
        if line.is_empty() {
            continue;
        }
        let parts: Vec<&str> = line.split_whitespace().collect();
        let a = Args { parts, line: ln + 1 };
        let def = match a.parts[0] {
            "spawn" => Def::Spawn { pos: a.v3(1)?, yaw: a.f_or(4, 0.0)?.to_radians() },
            "sea" => Def::Sea { y: a.f(1)? },
            "cake" => {
                let style = match a.parts.get(7).copied().unwrap_or("pink") {
                    "choco" => CakeStyle::Choco,
                    "mint" => CakeStyle::Mint,
                    "lemon" => CakeStyle::Lemon,
                    _ => CakeStyle::Pink,
                };
                Def::Cake { pos: a.v3(1)?, size: a.v3(4)?, style }
            }
            "cookie" => Def::Cookie { pos: a.v3(1)?, r: a.f(4)?, h: a.f(5)? },
            "choco" => Def::Choco { pos: a.v3(1)?, size: a.v3(4)? },
            "wafer" => Def::Wafer {
                pos: a.v3(1)?,
                size: a.v3(4)?,
                delta: a.v3(7)?,
                period: a.f(10)?.max(0.1),
                phase: a.f_or(11, 0.0)?,
            },
            "cherry" => Def::Cherry(a.v3(1)?),
            "ice" => Def::Ice(a.v3(1)?),
            "donut" => Def::Donut { pos: a.v3(1)?, delta: Vec2::new(a.f(4)?, a.f(5)?), speed: a.f(6)? },
            "mallow" => Def::Mallow(a.v3(1)?),
            "spinner" => Def::Spinner { pos: a.v3(1)?, length: a.f(4)?, speed: a.f(5)? },
            "checkpoint" => Def::Checkpoint(a.v3(1)?),
            "fridge" => Def::Fridge {
                pos: a.v3(1)?,
                yaw: a.f_or(4, 0.0)?.to_radians(),
                style: match a.parts.get(5).copied() {
                    Some("freezer") => GoalStyle::Freezer,
                    Some("party") => GoalStyle::Party,
                    Some("bread") => GoalStyle::Bread,
                    _ => GoalStyle::Kitchen,
                },
            },
            "theme" => Def::Theme(match a.parts.get(1).copied().unwrap_or("") {
                "kitchen" => Theme::Kitchen,
                "fridge" => Theme::Fridge,
                "freezer" => Theme::Freezer,
                "buffet" => Theme::Buffet,
                other => return Err(format!("Zeile {}: unbekanntes Thema '{}'", a.line, other)),
            }),
            "block" => Def::Block {
                pos: a.v3(1)?,
                size: a.v3(4)?,
                side: a.color(7)?,
                top: a.color(8)?,
                slippery: a.flag("glatt"),
                motion: None,
            },
            "mblock" => Def::Block {
                pos: a.v3(1)?,
                size: a.v3(4)?,
                side: a.color(7)?,
                top: a.color(8)?,
                slippery: false,
                motion: Some((a.v3(9)?, a.f(12)?.max(0.1), a.f_or(13, 0.0)?)),
            },
            "cyl" => Def::Cyl {
                pos: a.v3(1)?,
                r: a.f(4)?,
                h: a.f(5)?,
                side: a.color(6)?,
                top: a.color(7)?,
                slippery: a.flag("glatt"),
            },
            "deko" => match a.parts.get(1).copied().unwrap_or("") {
                "box" => Def::PropBox { pos: a.v3(2)?, size: a.v3(5)?, color: a.color(8)? },
                "cyl" => Def::PropCyl { pos: a.v3(2)?, r: a.f(5)?, h: a.f(6)?, color: a.color(7)? },
                other => return Err(format!("Zeile {}: unbekannte Deko '{}'", a.line, other)),
            },
            "cocoa" => Def::Cocoa(a.v3(1)?),
            "hotplate" => Def::HotPlate {
                pos: a.v3(1)?,
                r: a.f(4)?,
                period: a.f_or(5, 3.0)?.max(0.5),
                phase: a.f_or(6, 0.0)?,
            },
            "crumble" => Def::Crumble { pos: a.v3(1)?, size: a.v3(4)?, side: a.color(7)?, top: a.color(8)? },
            "cane" => Def::Cane { pos: a.v3(1)?, h: a.f(4)? },
            "lolly" => Def::Lolly {
                pos: a.v3(1)?,
                h: a.f(4)?,
                color: u32::from_str_radix(a.parts.get(5).copied().unwrap_or("ff5080").trim_start_matches("0x"), 16)
                    .map_err(|_| format!("Zeile {}: ungültige Farbe", a.line))?,
            },
            "cupcake" => Def::Cupcake { pos: a.v3(1)?, s: a.f(4)? },
            "cloud" => Def::Cloud { pos: a.v3(1)?, s: a.f(4)? },
            "hint" => {
                // Text = alles nach den vier Zahlen (Originalschreibweise erhalten).
                let mut rest = line;
                for _ in 0..5 {
                    rest = rest.trim_start();
                    rest = &rest[rest.find(char::is_whitespace).unwrap_or(rest.len())..];
                }
                Def::Hint { pos: a.v3(1)?, r: a.f(4)?, text: rest.trim().to_string() }
            }
            "route" => Def::Route(match a.parts.get(1).copied().unwrap_or("") {
                "walk" => RouteStep::Walk(Vec2::new(a.f(2)?, a.f(3)?)),
                "jump" => RouteStep::Jump(Vec2::new(a.f(2)?, a.f(3)?)),
                "air" => RouteStep::Air(Vec2::new(a.f(2)?, a.f(3)?)),
                "wait" => RouteStep::Wait(a.f(2)?),
                "waitplat" => RouteStep::WaitPlat { idx: a.f(2)? as usize, s: a.f(3)?, tol: a.f(4)? },
                other => return Err(format!("Zeile {}: unbekannter Routenschritt '{}'", a.line, other)),
            }),
            other => return Err(format!("Zeile {}: unbekannter Typ '{}'", a.line, other)),
        };
        defs.push(def);
    }
    Ok(LevelDef { defs })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_builtin_level() {
        let l = parse(BUILTIN).expect("eingebautes Level muss parsen");
        assert!(l.defs.iter().any(|d| matches!(d, Def::Spawn { .. })));
        assert!(l.defs.iter().any(|d| matches!(d, Def::Fridge { .. })));
        assert!(l.defs.iter().filter(|d| matches!(d, Def::Cherry(_))).count() >= 20);
    }

    #[test]
    fn hint_keeps_text_and_errors_have_line_numbers() {
        let l = parse("hint 1 2 3 4  Hallo Welt: Springen!").unwrap();
        assert_eq!(
            l.defs[0],
            Def::Hint { pos: Vec3::new(1.0, 2.0, 3.0), r: 4.0, text: "Hallo Welt: Springen!".into() }
        );
        let e = parse("\ncake 1 2").unwrap_err();
        assert!(e.contains("Zeile 2"), "{e}");
    }
}
