//! Q2: Wetter (eigene App; die ColibriOS-Weather braucht einen
//! OpenWeatherMap-Schlüssel – PLAN Q2). Daten von Open-Meteo (frei, ohne
//! API-Schlüssel): aktuelles Wetter und Vorschau für 5 Tage. Ort per Suche
//! (Open-Meteo-Geocoding) über die Bildschirmtastatur, gespeichert in den
//! Einstellungen (`konfig`, Schlüssel wetter_*); die Einstellungen starten
//! die Suche mit dem Argument „ort“. Deutsch/Englisch, verständliche Meldung
//! ohne WLAN. Abruf über `kern::abruf` (Board: HTTPS wie das Update).

use kern::sprache::t;
use kern::sys;

const FX: i32 = 20;
const FY: i32 = 8;
const FW: i32 = 600;
const FH: i32 = 300;
const HG: u32 = 0xEEF2F7;
const K_ORT: u32 = 2;
const K_NEU: u32 = 3;
const K_ZURUECK: u32 = 4;
const K_TREFFER: u32 = 10;

// ------------------------------------------------------------ JSON (klein)

/// Kleiner JSON-Wert (nur was Open-Meteo liefert).
#[derive(Clone, Debug, PartialEq)]
pub enum J {
    Null,
    Bool(bool),
    Zahl(f64),
    Text(String),
    Liste(Vec<J>),
    Objekt(Vec<(String, J)>),
}

impl J {
    pub fn feld(&self, k: &str) -> Option<&J> {
        match self {
            J::Objekt(v) => v.iter().find(|(n, _)| n == k).map(|(_, w)| w),
            _ => None,
        }
    }
    pub fn zahl(&self) -> Option<f64> {
        if let J::Zahl(z) = self { Some(*z) } else { None }
    }
    pub fn text(&self) -> Option<&str> {
        if let J::Text(s) = self { Some(s) } else { None }
    }
    pub fn liste(&self) -> &[J] {
        if let J::Liste(v) = self { v } else { &[] }
    }
}

/// JSON lesen; `None` bei Fehlern.
pub fn json(s: &str) -> Option<J> {
    let b = s.as_bytes();
    let mut i = 0;
    let w = wert(b, &mut i)?;
    leer(b, &mut i);
    (i == b.len()).then_some(w)
}

fn leer(b: &[u8], i: &mut usize) {
    while *i < b.len() && b[*i].is_ascii_whitespace() {
        *i += 1;
    }
}

fn wert(b: &[u8], i: &mut usize) -> Option<J> {
    leer(b, i);
    match *b.get(*i)? {
        b'{' => {
            *i += 1;
            let mut v = Vec::new();
            leer(b, i);
            if b.get(*i) == Some(&b'}') {
                *i += 1;
                return Some(J::Objekt(v));
            }
            loop {
                leer(b, i);
                let J::Text(k) = wert(b, i)? else { return None };
                leer(b, i);
                if b.get(*i) != Some(&b':') {
                    return None;
                }
                *i += 1;
                v.push((k, wert(b, i)?));
                leer(b, i);
                match b.get(*i)? {
                    b',' => *i += 1,
                    b'}' => {
                        *i += 1;
                        return Some(J::Objekt(v));
                    }
                    _ => return None,
                }
            }
        }
        b'[' => {
            *i += 1;
            let mut v = Vec::new();
            leer(b, i);
            if b.get(*i) == Some(&b']') {
                *i += 1;
                return Some(J::Liste(v));
            }
            loop {
                v.push(wert(b, i)?);
                leer(b, i);
                match b.get(*i)? {
                    b',' => *i += 1,
                    b']' => {
                        *i += 1;
                        return Some(J::Liste(v));
                    }
                    _ => return None,
                }
            }
        }
        b'"' => {
            *i += 1;
            let mut s = Vec::new();
            loop {
                let c = *b.get(*i)?;
                *i += 1;
                match c {
                    b'"' => return String::from_utf8(s).ok().map(J::Text),
                    b'\\' => {
                        let e = *b.get(*i)?;
                        *i += 1;
                        match e {
                            b'n' => s.push(b'\n'),
                            b't' => s.push(b'\t'),
                            b'u' => {
                                let h = std::str::from_utf8(b.get(*i..*i + 4)?).ok()?;
                                *i += 4;
                                let c = char::from_u32(u32::from_str_radix(h, 16).ok()?).unwrap_or('?');
                                let mut buf = [0u8; 4];
                                s.extend_from_slice(c.encode_utf8(&mut buf).as_bytes());
                            }
                            x => s.push(x),
                        }
                    }
                    x => s.push(x),
                }
            }
        }
        b't' if b[*i..].starts_with(b"true") => {
            *i += 4;
            Some(J::Bool(true))
        }
        b'f' if b[*i..].starts_with(b"false") => {
            *i += 5;
            Some(J::Bool(false))
        }
        b'n' if b[*i..].starts_with(b"null") => {
            *i += 4;
            Some(J::Null)
        }
        _ => {
            let a = *i;
            while *i < b.len() && matches!(b[*i], b'-' | b'+' | b'.' | b'e' | b'E' | b'0'..=b'9') {
                *i += 1;
            }
            std::str::from_utf8(&b[a..*i]).ok()?.parse().ok().map(J::Zahl)
        }
    }
}

// ------------------------------------------------------------ Daten

/// Ein Ort (Suche bzw. Einstellungen).
#[derive(Clone, Debug, PartialEq)]
pub struct Ort {
    pub name: String,
    pub lat: f64,
    pub lon: f64,
}

/// Ein Tag der Vorschau.
#[derive(Clone, Debug, PartialEq)]
pub struct Tag {
    pub datum: String,
    pub code: u32,
    pub max: f64,
    pub min: f64,
}

/// Aktuelles Wetter und Vorschau.
#[derive(Clone, Debug, PartialEq)]
pub struct Wetter {
    pub temp: f64,
    pub code: u32,
    pub wind: f64,
    pub feuchte: f64,
    pub tage: Vec<Tag>,
}

/// Prozent-Kodierung für die Suche.
fn kodiert(s: &str) -> String {
    s.bytes().map(|b| if b.is_ascii_alphanumeric() || b == b'-' || b == b'.' { (b as char).to_string() } else { format!("%{b:02X}") }).collect()
}

pub fn such_url(name: &str) -> String {
    format!("https://geocoding-api.open-meteo.com/v1/search?name={}&count=5&language={}&format=json", kodiert(name.trim()), kern::sprache::kuerzel(kern::sprache::aktuell()))
}

pub fn wetter_url(o: &Ort) -> String {
    format!(
        "https://api.open-meteo.com/v1/forecast?latitude={:.4}&longitude={:.4}&current=temperature_2m,relative_humidity_2m,weather_code,wind_speed_10m&daily=weather_code,temperature_2m_max,temperature_2m_min&timezone=auto&forecast_days=5",
        o.lat, o.lon
    )
}

/// Treffer der Ortssuche („Name, Region, Land“).
pub fn orte_lesen(s: &str) -> Option<Vec<Ort>> {
    let j = json(s)?;
    Some(
        j.feld("results")
            .map(|r| r.liste())
            .unwrap_or(&[])
            .iter()
            .filter_map(|e| {
                let mut name = e.feld("name")?.text()?.to_string();
                for k in ["admin1", "country"] {
                    if let Some(x) = e.feld(k).and_then(J::text)
                        && !x.is_empty()
                        && x != name
                    {
                        name = format!("{name}, {x}");
                    }
                }
                Some(Ort { name, lat: e.feld("latitude")?.zahl()?, lon: e.feld("longitude")?.zahl()? })
            })
            .collect(),
    )
}

/// Antwort der Vorhersage lesen.
pub fn wetter_lesen(s: &str) -> Option<Wetter> {
    let j = json(s)?;
    let c = j.feld("current")?;
    let d = j.feld("daily")?;
    let zahlen = |k: &str| -> Vec<f64> { d.feld(k).map(|v| v.liste().iter().filter_map(J::zahl).collect()).unwrap_or_default() };
    let (codes, max, min) = (zahlen("weather_code"), zahlen("temperature_2m_max"), zahlen("temperature_2m_min"));
    let tage = d
        .feld("time")?
        .liste()
        .iter()
        .enumerate()
        .filter_map(|(i, t)| Some(Tag { datum: t.text()?.to_string(), code: *codes.get(i)? as u32, max: *max.get(i)?, min: *min.get(i)? }))
        .collect();
    Some(Wetter {
        temp: c.feld("temperature_2m")?.zahl()?,
        code: c.feld("weather_code")?.zahl()? as u32,
        wind: c.feld("wind_speed_10m").and_then(J::zahl).unwrap_or(0.0),
        feuchte: c.feld("relative_humidity_2m").and_then(J::zahl).unwrap_or(0.0),
        tage,
    })
}

/// Art des Wetters (WMO-Code), für Text und Bild.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Art {
    Klar,
    Wolkig,
    Bedeckt,
    Nebel,
    Niesel,
    Regen,
    Schnee,
    Gewitter,
}

pub fn art(code: u32) -> Art {
    match code {
        0 | 1 => Art::Klar,
        2 => Art::Wolkig,
        3 => Art::Bedeckt,
        45 | 48 => Art::Nebel,
        51..=57 => Art::Niesel,
        61..=67 | 80..=82 => Art::Regen,
        71..=77 | 85 | 86 => Art::Schnee,
        95..=99 => Art::Gewitter,
        _ => Art::Bedeckt,
    }
}

pub fn art_text(code: u32) -> &'static str {
    match art(code) {
        Art::Klar => t("Clear", "Klar"),
        Art::Wolkig => t("Partly cloudy", "Teils bewölkt"),
        Art::Bedeckt => t("Cloudy", "Bewölkt"),
        Art::Nebel => t("Fog", "Nebel"),
        Art::Niesel => t("Drizzle", "Nieselregen"),
        Art::Regen => t("Rain", "Regen"),
        Art::Schnee => t("Snow", "Schnee"),
        Art::Gewitter => t("Thunderstorm", "Gewitter"),
    }
}

/// Wochentag (0 = Montag) aus „JJJJ-MM-TT“.
pub fn wochentag(datum: &str) -> Option<usize> {
    let mut t = datum.split('-').map(|x| x.parse::<i64>().ok());
    let (j, m, d) = (t.next()??, t.next()??, t.next()??);
    // Tage seit 1970-01-01 (Donnerstag)
    let (j2, m2) = if m <= 2 { (j - 1, m + 9) } else { (j, m - 3) };
    let era = j2.div_euclid(400);
    let yoe = j2 - era * 400;
    let doy = (153 * m2 + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    let tage = era * 146_097 + doe - 719_468;
    Some(((tage + 3).rem_euclid(7)) as usize)
}

fn tagname(i: usize) -> &'static str {
    const EN: [&str; 7] = ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"];
    const DE: [&str; 7] = ["Mo", "Di", "Mi", "Do", "Fr", "Sa", "So"];
    t(EN[i % 7], DE[i % 7])
}

// ------------------------------------------------------------ Zeichnen

fn text(x: i32, y: i32, s: &str, farbe: u32, gross: bool) {
    let f = if gross { 0x3100_0000 } else { 0x3000_0000 };
    sys::text((x as u32) << 16 | y as u32 & 0xFFFF, f | farbe & 0xFF_FFFF, s.as_bytes(), s.chars().count() as u32, 0);
}

fn rechteck(x: i32, y: i32, w: i32, h: i32, c: u32) {
    if w > 0 && h > 0 {
        sys::rechteck((x as u32) << 16 | w as u32, (y as u32) << 16 | h as u32, c);
    }
}

fn kreis(cx: i32, cy: i32, r: i32, c: u32) {
    for dy in -r..=r {
        let dx = ((r * r - dy * dy) as f64).sqrt() as i32;
        rechteck(cx - dx, cy + dy, 2 * dx + 1, 1, c);
    }
}

fn wolke(x: i32, y: i32, g: i32, c: u32) {
    kreis(x + g * 3 / 8, y + g / 2, g / 4, c);
    kreis(x + g * 5 / 8, y + g * 2 / 5, g * 3 / 10, c);
    rechteck(x + g / 8, y + g / 2, g * 3 / 4, g / 4, c);
}

/// Wettersymbol in einem Quadrat der Grösse `g`.
fn symbol(x: i32, y: i32, g: i32, code: u32) {
    let a = art(code);
    let sonne = 0xF2B705;
    let grau = if matches!(a, Art::Gewitter | Art::Regen) { 0x7A8494 } else { 0xA9B4C2 };
    match a {
        Art::Klar => kreis(x + g / 2, y + g / 2, g * 3 / 10, sonne),
        Art::Wolkig => {
            kreis(x + g * 2 / 3, y + g / 3, g / 4, sonne);
            wolke(x, y + g / 8, g, grau);
        }
        Art::Bedeckt => wolke(x, y, g, grau),
        Art::Nebel => {
            for i in 0..3 {
                rechteck(x + g / 8, y + g / 4 + i * g / 5, g * 3 / 4, (g / 12).max(2), 0x9AA4B0);
            }
        }
        Art::Niesel | Art::Regen | Art::Schnee | Art::Gewitter => {
            wolke(x, y - g / 8, g, grau);
            let n = if a == Art::Niesel { 2 } else { 3 };
            for i in 0..n {
                let px = x + g / 4 + i * g / 4;
                let py = y + g * 3 / 4;
                match a {
                    Art::Schnee => kreis(px, py, (g / 16).max(2), 0x8FB8E0),
                    Art::Gewitter if i == 1 => {
                        for k in 0..(g / 5).max(3) {
                            rechteck(px - k / 2, py - g / 10 + k, (g / 16).max(2), 1, sonne);
                        }
                    }
                    _ => rechteck(px, py - g / 16, (g / 24).max(2), (g / 6).max(3), 0x3A7AC0),
                }
            }
        }
    }
}

fn knopf(id: u32, x: i32, y: i32, w: i32, h: i32, s: &str) {
    sys::knopf((x as u32) << 16 | (w - 1) as u32, (y as u32) << 16 | (h - 1) as u32, id, 0x3A7AC0);
    text(x + (w - s.chars().count() as i32 * 8) / 2, y + (h - 16) / 2, s, 0xFFFFFF, false);
}

/// Zustand des Programms.
#[derive(Default)]
struct Zustand {
    ort: Option<Ort>,
    wetter: Option<Wetter>,
    meldung: Option<String>,
    /// Ortssuche: Treffer zum Antippen
    treffer: Vec<Ort>,
}

fn temp(v: f64) -> String {
    format!("{}°", v.round() as i64)
}

fn zeichnen(z: &Zustand) {
    sys::neuzeichnen(1);
    sys::fenster((FX as u32) << 16 | (FW - 1) as u32, (FY as u32) << 16 | (FH - 1) as u32, 0x33 << 24 | HG, 0, Some(t("Weather", "Wetter")));
    let (w, _) = (FW - 10, FH);
    if !z.treffer.is_empty() {
        text(12, 8, t("Choose a place:", "Ort wählen:"), 0x202020, false);
        // Zurück zur Wetteranzeige ohne Auswahl (wie in den anderen Programmen)
        knopf(K_ZURUECK, w - 120, 0, 110, 28, t("Back", "Zurück"));
        for (i, o) in z.treffer.iter().enumerate() {
            let y = 32 + i as i32 * 44;
            sys::knopf(12 << 16 | (w - 25) as u32, (y as u32) << 16 | 39, K_TREFFER + i as u32, 0xFFFFFF);
            let n: String = o.name.chars().take(((w - 40) / 8) as usize).collect();
            text(22, y + 12, &n, 0x1A4A8A, false);
        }
        sys::neuzeichnen(2);
        return;
    }
    match &z.ort {
        None => text(12, 12, t("No place chosen yet.", "Noch kein Ort gewählt."), 0x404040, false),
        Some(o) => {
            let n: String = o.name.chars().take(((w - 260) / 8) as usize).collect();
            text(12, 10, &n, 0x202020, false);
        }
    }
    if let Some(wt) = &z.wetter {
        symbol(16, 40, 96, wt.code);
        text(130, 50, &temp(wt.temp), 0x202020, true);
        text(130, 90, art_text(wt.code), 0x404040, false);
        text(130, 112, &format!("{} {} km/h   {} {} %", t("Wind", "Wind"), wt.wind.round() as i64, t("Humidity", "Feuchte"), wt.feuchte.round() as i64), 0x606060, false);
        // Vorschau
        let sw = (w - 24) / 5;
        for (i, tag) in wt.tage.iter().take(5).enumerate() {
            let x = 12 + i as i32 * sw;
            let y = 146;
            rechteck(x + 2, y, sw - 6, 92, 0xFFFFFF);
            let name = if i == 0 { t("Today", "Heute").to_string() } else { wochentag(&tag.datum).map_or_else(|| tag.datum.clone(), |d| tagname(d).to_string()) };
            text(x + (sw - name.chars().count() as i32 * 8) / 2, y + 6, &name, 0x202020, false);
            symbol(x + (sw - 40) / 2, y + 26, 40, tag.code);
            let mm = format!("{} / {}", temp(tag.max), temp(tag.min));
            text(x + (sw - mm.chars().count() as i32 * 8) / 2, y + 70, &mm, 0x404040, false);
        }
    }
    if let Some(m) = &z.meldung {
        let n: String = m.chars().take(((w - 24) / 8) as usize).collect();
        text(12, if z.wetter.is_some() { 136 } else { 44 }, &n, 0xB03020, false);
    }
    knopf(K_ORT, w - 236, 4, 110, 32, if z.ort.is_some() { t("Place", "Ort") } else { t("Choose place", "Ort wählen") });
    knopf(K_NEU, w - 120, 4, 110, 32, t("Refresh", "Neu laden"));
    sys::neuzeichnen(2);
}

/// Fehlertext des Abrufs, ohne WLAN verständlich.
fn abruf_fehler(e: &str) -> String {
    if kern::wlan::verbunden_mit().is_some() {
        format!("{}: {e}", t("No weather data", "Keine Wetterdaten"))
    } else {
        t("No Wi-Fi - connect in Settings > Wi-Fi", "Kein WLAN - in Einstellungen > WLAN verbinden").to_string()
    }
}

fn laden(z: &mut Zustand) {
    let Some(o) = z.ort.clone() else { return };
    z.meldung = Some(t("Loading ...", "Lade ...").to_string());
    zeichnen(z);
    match kern::abruf::text(&wetter_url(&o)) {
        Ok((200, s)) => match wetter_lesen(&s) {
            Some(w) => {
                z.wetter = Some(w);
                z.meldung = None;
            }
            None => z.meldung = Some(t("Weather data not readable", "Wetterdaten nicht lesbar").into()),
        },
        Ok((st, _)) => z.meldung = Some(format!("{} (HTTP {st})", t("Weather service not available", "Wetterdienst nicht erreichbar"))),
        Err(e) => z.meldung = Some(abruf_fehler(&e)),
    }
    sys::debug_text(&format!("Wetter: {} – {:?}", o.name, z.wetter.as_ref().map(|w| (w.temp, w.code, w.tage.len()))));
}

fn suchen(z: &mut Zustand) {
    let Some(q) = crate::dateimanager::name_dialog(t("Search place", "Ort suchen"), "") else { return };
    z.meldung = Some(t("Searching ...", "Suche ...").to_string());
    zeichnen(z);
    // Board 25.11.: „zuerich“ findet Open-Meteo nicht (nur „Zürich“) – ohne
    // Treffer noch einmal mit ue/ae/oe als ü/ä/ö
    for (i, frage) in suchvarianten(&q).iter().enumerate() {
        match kern::abruf::text(&such_url(frage)) {
            Ok((200, s)) => match orte_lesen(&s) {
                Some(v) if !v.is_empty() => {
                    z.treffer = v;
                    z.meldung = None;
                    return;
                }
                _ => z.meldung = Some(format!("{} \"{q}\"", t("No place found for", "Kein Ort gefunden für"))),
            },
            Ok((st, _)) => {
                z.meldung = Some(format!("{} (HTTP {st})", t("Search not available", "Suche nicht erreichbar")));
                return;
            }
            Err(e) => {
                z.meldung = Some(abruf_fehler(&e));
                return;
            }
        }
        sys::debug_text(&format!("Wetter: Suche {i} \"{frage}\" ohne Treffer"));
    }
}

/// Suchtexte: wie getippt, dann ue/ae/oe als Umlaut (falls anders).
pub fn suchvarianten(q: &str) -> Vec<String> {
    let mut v = vec![q.to_string()];
    let u = q.replace("ue", "ü").replace("ae", "ä").replace("oe", "ö").replace("Ue", "Ü").replace("Ae", "Ä").replace("Oe", "Ö");
    if u != q {
        v.push(u);
    }
    v
}

/// Programm „wetter“ (Argument „ort“: gleich die Ortssuche, aus den
/// Einstellungen).
pub fn programm() {
    let mut z = Zustand { ort: crate::konfig::wetter_ort(), ..Default::default() };
    sys::maske(0x07);
    zeichnen(&z);
    if kern::argumente().trim() == "ort" || z.ort.is_none() {
        suchen(&mut z);
        zeichnen(&z);
    } else {
        laden(&mut z);
        zeichnen(&z);
    }
    loop {
        match sys::warte_ereignis() {
            1 => zeichnen(&z),
            3 => {
                let id = sys::knopf_id() >> 8;
                match id {
                    1 => return,
                    K_ORT => {
                        suchen(&mut z);
                        zeichnen(&z);
                    }
                    K_NEU => {
                        laden(&mut z);
                        zeichnen(&z);
                    }
                    K_ZURUECK => {
                        z.treffer.clear();
                        zeichnen(&z);
                    }
                    i if i >= K_TREFFER && ((i - K_TREFFER) as usize) < z.treffer.len() => {
                        let o = z.treffer[(i - K_TREFFER) as usize].clone();
                        z.treffer.clear();
                        crate::konfig::wetter_ort_setzen(&o);
                        z.ort = Some(o);
                        z.wetter = None;
                        laden(&mut z);
                        zeichnen(&z);
                    }
                    _ => {}
                }
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn json_und_open_meteo() {
        let s = r#"{"latitude":47.36,"current":{"time":"2026-11-25T15:00","temperature_2m":4.6,"relative_humidity_2m":81,"weather_code":61,"wind_speed_10m":12.3},"daily":{"time":["2026-11-25","2026-11-26","2026-11-27"],"weather_code":[61,3,0],"temperature_2m_max":[6.1,5.0,7.9],"temperature_2m_min":[1.2,-0.4,-2.0]}}"#;
        let w = wetter_lesen(s).expect("lesbar");
        assert_eq!((w.temp, w.code, w.feuchte), (4.6, 61, 81.0));
        assert_eq!(w.tage.len(), 3);
        assert_eq!(w.tage[1], Tag { datum: "2026-11-26".into(), code: 3, max: 5.0, min: -0.4 });
        let o = orte_lesen(r#"{"results":[{"name":"Zürich","latitude":47.36667,"longitude":8.55,"country":"Schweiz","admin1":"Zürich"},{"name":"Zurich","latitude":43.4,"longitude":-79.0,"country":"Kanada","admin1":"Ontario"}],"generationtime_ms":0.5}"#).unwrap();
        assert_eq!(o[0].name, "Zürich, Schweiz");
        assert_eq!(o[1].name, "Zurich, Ontario, Kanada");
        assert_eq!(orte_lesen(r#"{"generationtime_ms":0.3}"#), Some(vec![]));
        assert!(json("{\"a\":").is_none());
        assert_eq!(art(0), Art::Klar);
        assert_eq!(art(95), Art::Gewitter);
        assert_eq!(wochentag("2026-11-25"), Some(2)); // Mittwoch
        assert_eq!(wochentag("2024-02-29"), Some(3)); // Donnerstag
        assert!(such_url("St. Gallen").contains("name=St.%20Gallen"));
        assert_eq!(suchvarianten("zuerich"), vec!["zuerich".to_string(), "zürich".to_string()]);
        assert_eq!(suchvarianten("Bern"), vec!["Bern".to_string()]);
    }
}
