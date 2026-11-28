//! Updates über WLAN (U1, Auftrag 12.7): neueste Version vom GitHub-Release
//! holen. Die Firmware meldet ihre [`Quelle`] an (HTTPS mit
//! Zertifikatsprüfung, Prüfsumme, OTA in den zweiten ESP32-OS-Platz,
//! Rückfall per Bootloader-Rollback); am PC gibt es die [`Simulation`].
//! Versionsvergleich hier ([`neuer`]), damit er testbar ist.

use std::sync::{Mutex, OnceLock};

/// Eine gefundene neuere Version.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Freigabe {
    /// Tag des Releases, z. B. „v1.0.1“.
    pub version: String,
    /// Abbild (`esp32os.bin`).
    pub bild_url: String,
    /// Prüfsumme (`esp32os.bin.sha256`, SHA-256 hexadezimal).
    pub pruef_url: String,
}

/// Quelle der Updates (Board: GitHub über HTTPS).
pub trait Quelle: Send + Sync {
    /// Neueste Freigabe holen (ohne Vergleich mit der laufenden Version).
    fn neueste(&self) -> Result<Freigabe, String>;
    /// Herunterladen, Prüfsumme prüfen, in den zweiten Platz schreiben und
    /// als Startplatz setzen. `fortschritt` in Prozent. Danach startet die
    /// Firmware neu (Rückfall, wenn die neue Version sich nicht meldet).
    fn installieren(&self, f: &Freigabe, fortschritt: &mut dyn FnMut(u32)) -> Result<(), String>;
}

static QUELLE: OnceLock<Box<dyn Quelle>> = OnceLock::new();
/// Nur ein Vorgang zur Zeit.
static BETRIEB: Mutex<()> = Mutex::new(());

/// Quelle anmelden (Firmware, einmal).
pub fn quelle_setzen(q: Box<dyn Quelle>) -> bool {
    crate::kern_abschnitt(|| QUELLE.set(q).is_ok())
}

pub fn vorhanden() -> bool {
    QUELLE.get().is_some()
}

/// Zahlen einer Version: „v1.2.3“ → [1, 2, 3]; „1.0-dev (…)“ → [1, 0] +
/// Vorabversion. Leer, wenn nichts Zahlenartiges vorne steht.
fn teile(v: &str) -> (Vec<u32>, bool) {
    let v = v.trim().trim_start_matches(['v', 'V']);
    let kern_teil: String = v.chars().take_while(|c| c.is_ascii_digit() || *c == '.').collect();
    let zahlen: Vec<u32> = kern_teil.split('.').filter(|s| !s.is_empty()).filter_map(|s| s.parse().ok()).collect();
    let rest = &v[kern_teil.len()..];
    let vorab = rest.starts_with('-');
    (zahlen, vorab)
}

/// Ist `kandidat` neuer als `aktuell`? Fehlende Stellen zählen als 0, eine
/// Vorabversion (`-dev`, `-rc1`) ist älter als dieselbe Freigabe.
pub fn neuer(aktuell: &str, kandidat: &str) -> bool {
    let ((a, a_vorab), (k, k_vorab)) = (teile(aktuell), teile(kandidat));
    if k.is_empty() {
        return false;
    }
    let n = a.len().max(k.len());
    for i in 0..n {
        let (x, y) = (a.get(i).copied().unwrap_or(0), k.get(i).copied().unwrap_or(0));
        if y != x {
            return y > x;
        }
    }
    a_vorab && !k_vorab
}

/// Nach Updates suchen: `Ok(Some(..))` = neuere Version, `Ok(None)` =
/// aktuell.
fn pruefen_innen(aktuell: &str) -> Result<Option<Freigabe>, String> {
    let q = QUELLE.get().ok_or_else(|| "keine Update-Quelle".to_string())?;
    let _b = crate::kern_abschnitt(|| BETRIEB.lock().unwrap_or_else(|e| e.into_inner()));
    let f = q.neueste()?;
    Ok(neuer(aktuell, &f.version).then_some(f))
}

/// Freigabe installieren (Firmware startet danach neu).
fn installieren_innen(f: &Freigabe, fortschritt: &mut dyn FnMut(u32)) -> Result<(), String> {
    let q = QUELLE.get().ok_or_else(|| "keine Update-Quelle".to_string())?;
    let _b = crate::kern_abschnitt(|| BETRIEB.lock().unwrap_or_else(|e| e.into_inner()));
    q.installieren(f, fortschritt)
}

/// Werte eines Schlüssels im JSON (nur einfache Zeichenketten, wie die
/// GitHub-API sie liefert; Escapes kommen in Tags/URLs nicht vor).
fn json_texte<'a>(json: &'a str, schluessel: &str) -> Vec<&'a str> {
    let muster = format!("\"{schluessel}\"");
    let mut v = Vec::new();
    let mut rest = json;
    while let Some(i) = rest.find(&muster) {
        rest = &rest[i + muster.len()..];
        let Some(nach) = rest.trim_start().strip_prefix(':') else { continue };
        let Some(wert) = nach.trim_start().strip_prefix('"') else { continue };
        if let Some(ende) = wert.find('"') {
            v.push(&wert[..ende]);
        }
    }
    v
}

/// Antwort von `releases/latest` → Freigabe (Tag, `esp32os.bin`,
/// `esp32os.bin.sha256`). Nur HTTPS-Adressen werden angenommen.
pub fn freigabe_aus_json(json: &str) -> Result<Freigabe, String> {
    let version = json_texte(json, "tag_name").first().ok_or("Release ohne Tag")?.to_string();
    let urls = json_texte(json, "browser_download_url");
    let finde = |endung: &str| urls.iter().find(|u| u.starts_with("https://") && u.ends_with(endung)).map(|u| u.to_string());
    Ok(Freigabe {
        version,
        bild_url: finde("/esp32os.bin").ok_or("Release ohne esp32os.bin")?,
        pruef_url: finde("/esp32os.bin.sha256").ok_or("Release ohne esp32os.bin.sha256")?,
    })
}

// ------------------------------------------------------------ D3 App-Store

/// D3: Programme, die als eigenes Abbild im Release liegen (Doom, Quake):
/// Platz (Partition) und Dateiname im Release.
pub trait Abbilder: Send + Sync {
    /// Liegt ein gültiges Abbild im Platz? None = Platz fehlt / unbekannt.
    fn vorhanden(&self, platz: &str) -> Option<bool>;
    /// Aus dem neuesten Release laden (HTTPS, Zertifikat geprüft), SHA-256
    /// gegen `SHA256SUMS`, in den Platz schreiben – ohne Startplatz-Wechsel.
    fn installieren(&self, platz: &str, datei: &str, fortschritt: &mut dyn FnMut(u32)) -> Result<(), String>;
    /// Kopf des Abbilds löschen (startet danach nicht, gilt als entfernt).
    fn entfernen(&self, platz: &str) -> Result<(), String>;
}

static ABBILDER: OnceLock<Box<dyn Abbilder>> = OnceLock::new();

/// Quelle der Abbilder anmelden (Firmware bzw. Test, einmal).
pub fn abbilder_setzen(a: Box<dyn Abbilder>) -> bool {
    crate::kern_abschnitt(|| ABBILDER.set(a).is_ok())
}

pub fn abbild_vorhanden(platz: &str) -> Option<bool> {
    ABBILDER.get()?.vorhanden(platz)
}

/// Installieren (ein Vorgang zur Zeit, wie beim Update; geschützt).
pub fn abbild_installieren(platz: &str, datei: &str, fortschritt: &mut dyn FnMut(u32)) -> Result<(), String> {
    let a = ABBILDER.get().ok_or_else(|| crate::sprache::t("App store not available", "App-Store nicht verfügbar").to_string())?;
    let _g = crate::kern_abschnitt(|| BETRIEB.try_lock().ok()).ok_or_else(|| crate::sprache::t("Another download is running", "Ein anderer Download läuft").to_string())?;
    crate::geschuetzt(|| a.installieren(platz, datei, fortschritt))
}

pub fn abbild_entfernen(platz: &str) -> Result<(), String> {
    let a = ABBILDER.get().ok_or_else(|| crate::sprache::t("App store not available", "App-Store nicht verfügbar").to_string())?;
    let _g = crate::kern_abschnitt(|| BETRIEB.try_lock().ok()).ok_or_else(|| crate::sprache::t("Another download is running", "Ein anderer Download läuft").to_string())?;
    crate::geschuetzt(|| a.entfernen(platz))
}

/// URL der Release-Datei mit genau diesem Namen (nur HTTPS; `…/doomapp.bin`,
/// nie `…/guition-doomapp.bin`).
pub fn datei_url(json: &str, datei: &str) -> Option<String> {
    let endung = format!("/{datei}");
    json_texte(json, "browser_download_url").into_iter().find(|u| u.starts_with("https://") && u.ends_with(&endung)).map(|u| u.to_string())
}

/// Prüfsumme einer Datei aus `SHA256SUMS` (Zeilen „hex  name“ oder
/// „hex *name“).
pub fn sha256_fuer(summen: &str, datei: &str) -> Option<[u8; 32]> {
    summen.lines().find_map(|z| {
        let (hex, name) = z.trim().split_once(char::is_whitespace)?;
        (name.trim().trim_start_matches('*') == datei).then(|| sha256_aus_text(hex)).flatten()
    })
}

/// Prüfsummendatei (`sha256sum`-Format: 64 Hex-Ziffern, dann Name) → Bytes.
pub fn sha256_aus_text(t: &str) -> Option<[u8; 32]> {
    let hex: Vec<u8> = t.trim().bytes().take(64).collect();
    if hex.len() != 64 {
        return None;
    }
    let mut sha = [0u8; 32];
    for (i, b) in sha.iter_mut().enumerate() {
        *b = u8::from_str_radix(core::str::from_utf8(&hex[2 * i..2 * i + 2]).ok()?, 16).ok()?;
    }
    Some(sha)
}

/// Simulierte Quelle (PC-Tests): feste neueste Version; Installieren meldet
/// Fortschritt und merkt sich die Version.
pub struct Simulation {
    pub neueste: Freigabe,
    pub installiert: Mutex<Option<String>>,
}

impl Quelle for Simulation {
    fn neueste(&self) -> Result<Freigabe, String> {
        Ok(self.neueste.clone())
    }

    fn installieren(&self, f: &Freigabe, fortschritt: &mut dyn FnMut(u32)) -> Result<(), String> {
        for p in [0, 25, 50, 75, 100] {
            fortschritt(p);
        }
        let v = f.version.clone();
        crate::kern_abschnitt(|| *self.installiert.lock().unwrap_or_else(|e| e.into_inner()) = Some(v));
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::{datei_url, freigabe_aus_json, neuer, sha256_aus_text, sha256_fuer};

    /// D3: Datei genau nach Namen (nie die eines anderen Boards), Prüfsumme
    /// aus SHA256SUMS, nur HTTPS.
    #[test]
    fn app_store_datei_und_pruefsumme() {
        let json = r#"{"tag_name":"v1.0.5","assets":[{"browser_download_url":"https://x/guition-doomapp.bin"},{"browser_download_url":"https://x/doomapp.bin"},{"browser_download_url":"http://y/quakeapp.bin"},{"browser_download_url":"https://x/SHA256SUMS"}]}"#;
        assert_eq!(datei_url(json, "doomapp.bin").as_deref(), Some("https://x/doomapp.bin"));
        assert_eq!(datei_url(json, "quakeapp.bin"), None, "nur HTTPS");
        assert_eq!(datei_url(json, "fehlt.bin"), None);
        let a = "ab".repeat(32);
        let b = "cd".repeat(32);
        let summen = format!("{a}  guition-doomapp.bin\n{b}  doomapp.bin\n{a} *quakeapp.bin\n");
        assert_eq!(sha256_fuer(&summen, "doomapp.bin"), Some([0xCD; 32]));
        assert_eq!(sha256_fuer(&summen, "quakeapp.bin"), Some([0xAB; 32]));
        assert_eq!(sha256_fuer(&summen, "esp32os.bin"), None);
        assert_eq!(sha256_fuer("kaputt  doomapp.bin", "doomapp.bin"), None);
    }

    #[test]
    fn release_json_und_pruefsumme() {
        let json = r#"{"url":"x","tag_name": "v1.0.1","name":"ESP32-OS 1.0.1","assets":[
            {"name":"esp32os.bin.sha256","browser_download_url":"https://github.com/o/r/releases/download/v1.0.1/esp32os.bin.sha256"},
            {"name":"esp32os.bin","browser_download_url":"https://github.com/o/r/releases/download/v1.0.1/esp32os.bin"},
            {"name":"doom.zip","browser_download_url":"https://github.com/o/r/releases/download/v1.0.1/doom.zip"}]}"#;
        let f = freigabe_aus_json(json).unwrap();
        assert_eq!(f.version, "v1.0.1");
        assert!(f.bild_url.ends_with("/v1.0.1/esp32os.bin"));
        assert!(f.pruef_url.ends_with("/esp32os.bin.sha256"));
        // nur HTTPS
        let unsicher = json.replace("https://github.com/o/r/releases/download/v1.0.1/esp32os.bin\"", "http://x/esp32os.bin\"");
        assert!(freigabe_aus_json(&unsicher).is_err());
        assert!(freigabe_aus_json(r#"{"message":"Not Found"}"#).is_err());
        let sha = sha256_aus_text("5816b69f00112233445566778899aabbccddeeff00112233445566778899aabb  esp32os.bin
").unwrap();
        assert_eq!(sha[0], 0x58);
        assert_eq!(sha[31], 0xbb);
        assert!(sha256_aus_text("zu kurz").is_none());
        assert!(sha256_aus_text(&"zz".repeat(32)).is_none());
    }

    #[test]
    fn versionen_vergleichen() {
        assert!(neuer("1.0-dev (2026-11-21)", "v1.0.0"), "Freigabe neuer als Vorabversion");
        assert!(neuer("1.0-dev (2026-11-21)", "v1.0"));
        assert!(!neuer("1.0.0", "v1.0.0"));
        assert!(neuer("1.0.0", "v1.0.1"));
        assert!(neuer("1.9.9", "v1.10.0"), "Zahlen, nicht Text");
        assert!(!neuer("1.10.0", "v1.9.9"));
        assert!(!neuer("1.0.0", "v1.0.0-rc1"), "Vorabversion älter");
        assert!(neuer("1.0.0-rc1", "v1.0.0"));
        assert!(!neuer("2.0", "v1.99"));
        assert!(!neuer("1.0", "kaputt"), "kein Tag");
        assert!(!neuer("1.0", ""));
    }
}

// ---- Öffentliche Einstiege: geschützt (Zwangsbeenden, A6)

/// [`pruefen_innen`] im geschützten Abschnitt (`BETRIEB`, HTTPS und Flash).
pub fn pruefen(aktuell: &str) -> Result<Option<Freigabe>, String> {
    crate::geschuetzt(|| pruefen_innen(aktuell))
}

/// [`installieren_innen`] im geschützten Abschnitt (`BETRIEB`, HTTPS und Flash).
pub fn installieren(f: &Freigabe, fortschritt: &mut dyn FnMut(u32)) -> Result<(), String> {
    crate::geschuetzt(|| installieren_innen(f, fortschritt))
}
