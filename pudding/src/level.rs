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
//! fridge  x y z gier°                         # Ziel: Kühlschrank
//! cane    x y z h | lolly x y z h farbe | cupcake x y z s | cloud x y z s
//! hint    x y z r  Text bis zum Zeilenende    # Tipp-Text in der Nähe
//! route   walk x z | jump x z | air x z | wait s | waitplat i anteil toleranz   # Autopilot
//! ```

use spatz::math::{Vec2, Vec3};

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
    Fridge { pos: Vec3, yaw: f32 },
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

/// Das eingebaute Level.
pub const BUILTIN: &str = include_str!("../levels/level1.txt");

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
            "fridge" => Def::Fridge { pos: a.v3(1)?, yaw: a.f_or(4, 0.0)?.to_radians() },
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
