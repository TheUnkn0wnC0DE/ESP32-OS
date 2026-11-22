//! WLAN des Kerns (Block D). Die Firmware meldet ihren Treiber (ESP32-C6 über
//! esp_hosted) mit [`treiber_setzen`] an; am PC gibt es die [`Simulation`]
//! für die Tests. Programme (Einstellungen, Taskleiste, später die
//! Netzwerk-Systemfunktionen) fragen nur hier.
//!
//! * Zustand: [`wache`] (Firmware-Faden, alle 0,5 s) liest ihn beim Treiber
//!   und legt ihn ab; [`zustand`] ist dadurch billig (Taskleiste).
//! * Bekannte Netze (W4): zuletzt benutztes zuerst, höchstens
//!   [`MAX_BEKANNT`]; gespeichert im Treiber (Board: NVS).
//! * Automatisch verbinden: [`wache`] sucht, wenn nicht verbunden, nach
//!   bekannten Netzen und verbindet mit dem stärksten (Pause wächst bis
//!   5 min). „Trennen“ von Hand hält das an, bis wieder verbunden wird.
//!
//! Passwörter erscheinen nie im Log.
//!
//! Speicher: Programme rufen hier aus ihrem Faden auf; ihr Heap verschwindet
//! beim Programmende. Darum entsteht alles, was im Kern bleibt (Zustand,
//! bekannte Netze, Aufträge an den Treiber), im
//! [`kern_abschnitt`](crate::kern_abschnitt), und Kopien für das Programm
//! entstehen außerhalb davon (im Heap des Programms). Gesperrt wird immer im
//! Kern-Abschnitt (am Board legt das erste Sperren ein pthread-Objekt an).

use std::sync::{Mutex, OnceLock};

/// Zustand der Verbindung.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Zustand {
    /// Treiber läuft (Transport zum Koprozessor steht).
    pub bereit: bool,
    /// Verbindungsaufbau läuft.
    pub verbindet: bool,
    /// Verbunden und IP-Adresse erhalten.
    pub verbunden: bool,
    pub ip: [u8; 4],
    /// Signal in dBm (nur verbunden).
    pub rssi: i32,
    pub kanal: u8,
    /// 802.11ax (HE) ausgehandelt.
    pub he: bool,
    /// Grund der letzten Trennung (`wifi_err_reason_t`), 0 = keiner.
    pub grund: u16,
    /// Netzmaske, Gateway, DNS-Server (Funktion 76).
    pub maske: [u8; 4],
    pub gateway: [u8; 4],
    pub dns: [u8; 4],
    /// MAC-Adresse der Station (Funktion 76.0).
    pub mac: [u8; 6],
}

/// Sicherheit eines Netzes (`wifi_auth_mode_t`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Sicherheit {
    Offen,
    Wep,
    Wpa,
    Wpa2,
    Wpa3,
    Wpa2Wpa3,
    Firma,
    Andere,
}

impl Sicherheit {
    pub fn aus_authmode(m: i32) -> Sicherheit {
        match m {
            0 | 9 => Sicherheit::Offen, // offen, OWE (verschlüsselt ohne Passwort)
            1 => Sicherheit::Wep,
            2 => Sicherheit::Wpa,
            3 | 4 => Sicherheit::Wpa2,
            6 => Sicherheit::Wpa3,
            7 => Sicherheit::Wpa2Wpa3,
            5 | 10 | 12 | 13 => Sicherheit::Firma,
            _ => Sicherheit::Andere,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Sicherheit::Offen => crate::sprache::t("open", "offen"),
            Sicherheit::Wep => "WEP",
            Sicherheit::Wpa => "WPA",
            Sicherheit::Wpa2 => "WPA2",
            Sicherheit::Wpa3 => "WPA3",
            Sicherheit::Wpa2Wpa3 => "WPA2/WPA3",
            Sicherheit::Firma => "Enterprise",
            Sicherheit::Andere => "?",
        }
    }

    pub fn braucht_passwort(self) -> bool {
        self != Sicherheit::Offen
    }

    /// Verbindbar: offen oder WPA2/WPA3 mit Passwort. WEP und WPA (ohne 2)
    /// gelten als unsicher und werden abgelehnt (Schwelle WPA2 im Treiber),
    /// Enterprise braucht Zertifikate/Benutzer (nicht vorgesehen).
    pub fn unterstuetzt(self) -> bool {
        matches!(self, Sicherheit::Offen | Sicherheit::Wpa2 | Sicherheit::Wpa3 | Sicherheit::Wpa2Wpa3)
    }
}

/// Ein gefundenes Netz.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Netz {
    pub ssid: String,
    pub rssi: i32,
    pub kanal: u8,
    pub sicherheit: Sicherheit,
    /// Zugangspunkt kann 802.11ax.
    pub ax: bool,
}

/// Signalstärke in Balken 0..=4.
pub fn balken(rssi: i32) -> u8 {
    match rssi {
        r if r >= -55 => 4,
        r if r >= -67 => 3,
        r if r >= -75 => 2,
        r if r >= -85 => 1,
        _ => 0,
    }
}

/// Text zum Trennungsgrund (`wifi_err_reason_t`).
pub fn grund_text(g: u16) -> &'static str {
    use crate::sprache::t;
    match g {
        0 => "",
        2 | 15 | 202 | 204 => t("Wrong password?", "Falsches Passwort?"),
        201 => t("Network not found", "Netz nicht gefunden"),
        203 | 205 => t("Connection failed", "Verbindung fehlgeschlagen"),
        210 | 211 => t("Security not supported", "Sicherheit nicht unterstützt"),
        8 => t("Disconnected", "Getrennt"),
        _ => t("Connection lost", "Verbindung verloren"),
    }
}

/// Treiber der Plattform (Board: ESP32-C6, PC: [`Simulation`]).
pub trait Treiber: Send + Sync {
    /// Aktueller Zustand (Board: kann den Koprozessor fragen, nur aus [`wache`]).
    fn zustand(&self) -> Zustand;
    /// Netze suchen (blockiert einige Sekunden).
    fn suchen(&self) -> Result<Vec<Netz>, i32>;
    /// Verbindungsauftrag; das Ergebnis zeigt [`Treiber::zustand`].
    fn verbinden(&self, ssid: &str, passwort: &str) -> Result<(), i32>;
    fn trennen(&self);
    /// Gespeicherte bekannte Netze (SSID, Passwort).
    fn laden(&self) -> Vec<(String, String)>;
    fn sichern(&self, netze: &[(String, String)]) -> bool;
}

/// Höchstzahl bekannter Netze.
pub const MAX_BEKANNT: usize = 8;

static TREIBER: OnceLock<Box<dyn Treiber>> = OnceLock::new();

#[derive(Default)]
struct Lage {
    zustand: Zustand,
    /// Bekannte Netze, zuletzt benutztes zuerst (`None` = noch nicht geladen).
    bekannt: Option<Vec<(String, String)>>,
    /// „Trennen“ von Hand: nicht automatisch verbinden.
    von_hand_getrennt: bool,
    /// Nach erfolgreicher Verbindung merken (SSID, Passwort).
    merken_nach: Option<(String, String)>,
    /// Netz des letzten Verbindungsauftrags.
    ziel: Option<String>,
    /// Bekannte Netze geändert, noch nicht gesichert: Gesichert wird nur in
    /// [`wache`] (Firmware-Faden). Am Board darf Flash nur aus einem Task
    /// mit internem Stapel geschrieben werden; Programme haben PSRAM-Stapel.
    sichern_noetig: bool,
    /// Zähler der Verbindungsaufträge (automatischer Versuch erkennt, dass
    /// inzwischen von Hand verbunden wurde).
    auftrag: u32,
    naechster_versuch_ms: u64,
    pause_ms: u64,
    versuche: u32,
    /// Letzte Abfrage beim Treiber.
    abgefragt_ms: Option<u64>,
    /// Testnetz aus `wlan.local` (nur Test-Builds): (SSID, Passwort).
    test: Option<(String, String)>,
    /// Test-Builds: SSIDs in der Anzeige durch „Netz N“ ersetzt.
    anonym: Vec<String>,
}

static LAGE: Mutex<Option<Lage>> = Mutex::new(None);
/// Suchen und Verbinden nacheinander (der Koprozessor kann nur eines).
static BETRIEB: Mutex<()> = Mutex::new(());

/// Ändern: `f` läuft im Kern-Abschnitt (Belegungen im System-Heap).
fn mit_lage<R>(f: impl FnOnce(&mut Lage) -> R) -> R {
    crate::kern_abschnitt(|| {
        let mut g = LAGE.lock().unwrap_or_else(|e| e.into_inner());
        f(g.get_or_insert_with(Lage::default))
    })
}

/// Lesen: im Kern-Abschnitt sperren, `f` (Kopien) im Heap des Aufrufers.
fn lesen<R>(f: impl FnOnce(&Lage) -> R) -> R {
    let mut g = crate::kern_abschnitt(|| {
        let mut g = LAGE.lock().unwrap_or_else(|e| e.into_inner());
        g.get_or_insert_with(Lage::default);
        g
    });
    f(g.get_or_insert_with(Lage::default))
}

fn treiber() -> Option<&'static dyn Treiber> {
    TREIBER.get().map(|b| b.as_ref())
}

/// Treiber anmelden (einmal beim Start der Firmware).
pub fn treiber_setzen(t: Box<dyn Treiber>) -> bool {
    mit_lage(|_| ());
    crate::kern_abschnitt(|| TREIBER.set(t).is_ok())
}

/// Test-Builds (`bauen_flashen.ps1 -Wlan`): Testnetz aus `wlan.local`.
/// Die Anzeige zeigt dann keine echten SSIDs ([`anzeigename`]; Fotos für
/// die Nachweise), das Testnetz steht in der Liste oben, und der
/// Passwort-Dialog ist damit vorbelegt ([`test_passwort`]).
pub fn test_netz_setzen(ssid: &str, passwort: &str) {
    mit_lage(|l| l.test = Some((ssid.to_string(), passwort.to_string())));
}

/// Name für die Anzeige (Test-Builds: anonymisiert).
pub fn anzeigename(ssid: &str) -> String {
    // None = echter Name, Some(None) = Testnetz, Some(Some(i)) = „Netz i+1“
    let art = mit_lage(|l| match &l.test {
        None => None,
        Some((t, _)) if t == ssid => Some(None),
        Some(_) => Some(Some(match l.anonym.iter().position(|x| x == ssid) {
            Some(i) => i,
            None => {
                l.anonym.push(ssid.to_string());
                l.anonym.len() - 1
            }
        })),
    });
    match art {
        None => ssid.to_string(),
        Some(None) => crate::sprache::t("Test network (wlan.local)", "Testnetz (wlan.local)").to_string(),
        Some(Some(i)) => format!("{} {}", crate::sprache::t("Network", "Netz"), i + 1),
    }
}

/// Passwort-Vorgabe für das Testnetz (nur Test-Builds).
pub fn test_passwort(ssid: &str) -> Option<String> {
    lesen(|l| l.test.as_ref().filter(|(t, _)| t == ssid).map(|(_, p)| p.clone()))
}

/// Gibt es WLAN?
pub fn vorhanden() -> bool {
    treiber().is_some()
}

/// Zuletzt bekannter Zustand (billig, ohne Koprozessor).
pub fn zustand() -> Zustand {
    if treiber().is_none() {
        return Zustand::default();
    }
    mit_lage(|l| l.zustand)
}

/// SSID des verbundenen Netzes.
pub fn verbunden_mit() -> Option<String> {
    if treiber().is_none() {
        return None;
    }
    lesen(|l| if l.zustand.verbunden { l.ziel.clone() } else { None })
}

/// Netze suchen, nach Signal sortiert, jede SSID einmal (stärkster Punkt).
/// Läuft gerade ein Verbindungsaufbau, wird erst dessen Ende abgewartet
/// (höchstens 15 s) – der Funkteil lehnt eine Suche währenddessen ab.
fn suchen_innen() -> Result<Vec<Netz>, i32> {
    let t = treiber().ok_or(-1)?;
    let start = std::time::Instant::now();
    while zustand().verbindet && start.elapsed() < std::time::Duration::from_secs(15) {
        std::thread::sleep(std::time::Duration::from_millis(200));
    }
    let _b = crate::kern_abschnitt(|| BETRIEB.lock().unwrap_or_else(|e| e.into_inner()));
    let mut n = t.suchen()?;
    n.retain(|x| !x.ssid.is_empty());
    n.sort_by(|a, b| b.rssi.cmp(&a.rssi));
    let mut gesehen: Vec<String> = Vec::new();
    n.retain(|x| {
        if gesehen.contains(&x.ssid) {
            false
        } else {
            gesehen.push(x.ssid.clone());
            true
        }
    });
    // Test-Builds: Testnetz oben (Autopilot tippt die erste Zeile)
    if let Some(t) = lesen(|l| l.test.as_ref().map(|(s, _)| s.clone()))
        && let Some(i) = n.iter().position(|x| x.ssid == t)
    {
        let x = n.remove(i);
        n.insert(0, x);
    }
    Ok(n)
}

/// Mit einem Netz verbinden; nach Erfolg wird es gemerkt (`merken`).
fn verbinden_innen(ssid: &str, passwort: &str, merken: bool) -> Result<(), i32> {
    let t = treiber().ok_or(-1)?;
    // Erst BETRIEB, dann der neue Zustand, dann der Treiber: [`wache`] liest
    // den Treiber nur unter BETRIEB und kann so keinen veralteten Zustand
    // (noch das alte Netz verbunden) über den neuen Auftrag schreiben.
    let _b = crate::kern_abschnitt(|| BETRIEB.lock().unwrap_or_else(|e| e.into_inner()));
    mit_lage(|l| {
        l.von_hand_getrennt = false;
        l.auftrag = l.auftrag.wrapping_add(1);
        l.ziel = Some(ssid.to_string());
        l.merken_nach = merken.then(|| (ssid.to_string(), passwort.to_string()));
        l.zustand.verbindet = true;
        l.zustand.verbunden = false;
        l.zustand.grund = 0;
    });
    // Treiber im Kern-Abschnitt: was er sich merkt, gehört dem Kern
    let r = crate::kern_abschnitt(|| t.verbinden(ssid, passwort));
    if r.is_err() {
        mit_lage(|l| {
            l.zustand.verbindet = false;
            l.merken_nach = None;
        });
    }
    r
}

/// Trennen (von Hand: kein automatisches Wiederverbinden bis zum nächsten
/// [`verbinden`]).
fn trennen_innen() {
    if let Some(t) = treiber() {
        mit_lage(|l| {
            l.von_hand_getrennt = true;
            l.merken_nach = None;
        });
        crate::kern_abschnitt(|| t.trennen());
        mit_lage(|l| {
            l.zustand.verbunden = false;
            l.zustand.verbindet = false;
        });
    }
}

fn bekannt_laden(l: &mut Lage) -> &mut Vec<(String, String)> {
    if l.bekannt.is_none() {
        let mut v = treiber().map(|t| t.laden()).unwrap_or_default();
        v.truncate(MAX_BEKANNT);
        l.bekannt = Some(v);
    }
    l.bekannt.get_or_insert_with(Vec::new)
}

/// SSIDs der bekannten Netze (zuletzt benutztes zuerst).
pub fn bekannte() -> Vec<String> {
    mit_lage(|l| {
        bekannt_laden(l);
    });
    lesen(|l| l.bekannt.iter().flatten().map(|(s, _)| s.clone()).collect())
}

/// Passwort eines bekannten Netzes (zum Wiederverbinden aus der Liste).
pub fn passwort_von(ssid: &str) -> Option<String> {
    mit_lage(|l| {
        bekannt_laden(l);
    });
    lesen(|l| l.bekannt.iter().flatten().find(|(s, _)| s == ssid).map(|(_, p)| p.clone()))
}

/// Netz merken bzw. nach vorne holen (gesichert beim nächsten [`wache`]).
pub fn merken(ssid: &str, passwort: &str) -> bool {
    treiber().is_some()
        && mit_lage(|l| {
            let b = bekannt_laden(l);
            // Schon vorne mit gleichem Passwort: nichts schreiben (Flash
            // schonen; jedes automatische Wiederverbinden landet hier)
            if b.first().is_some_and(|(s, p)| s == ssid && p == passwort) {
                return true;
            }
            b.retain(|(x, _)| x != ssid);
            b.insert(0, (ssid.to_string(), passwort.to_string()));
            b.truncate(MAX_BEKANNT);
            l.sichern_noetig = true;
            true
        })
}

/// Bekanntes Netz vergessen (gesichert beim nächsten [`wache`]).
pub fn vergessen(ssid: &str) -> bool {
    treiber().is_some()
        && mit_lage(|l| {
            let b = bekannt_laden(l);
            let vorher = b.len();
            b.retain(|(x, _)| x != ssid);
            if b.len() != vorher {
                l.sichern_noetig = true;
            }
            true
        })
}

/// Geänderte bekannte Netze sichern (aus [`wache`], Firmware-Faden).
fn sichern_falls_noetig(t: &dyn Treiber) {
    mit_lage(|l| {
        if l.sichern_noetig {
            let liste = bekannt_laden(l).clone();
            if t.sichern(&liste) {
                l.sichern_noetig = false;
            }
        }
    });
}

/// Was [`wache`] getan hat (für das Log der Firmware).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Ereignis {
    Verbunden { ip: [u8; 4], rssi: i32, kanal: u8, he: bool },
    Getrennt { grund: u16 },
    /// Automatischer Versuch mit dem bekannten Netz Nr. `nr` (Liste, 0 = zuletzt benutzt).
    Versuch { nr: usize },
    KeinBekanntesNetz,
}

/// Zustand alle 0,5 s nachführen und automatisch verbinden (Firmware-Faden).
/// `jetzt_ms`: monotone Zeit. Gibt aus, was sich getan hat.
pub fn wache(jetzt_ms: u64) -> Vec<Ereignis> {
    let mut ereignisse = Vec::new();
    let Some(t) = treiber() else { return ereignisse };
    // Treiber lesen und übernehmen unter BETRIEB (siehe `verbinden`)
    let betrieb = crate::kern_abschnitt(|| BETRIEB.lock().unwrap_or_else(|e| e.into_inner()));
    let neu = crate::kern_abschnitt(|| t.zustand());
    let (alt, merken_nach) = mit_lage(|l| {
        let alt = l.zustand;
        l.zustand = neu;
        l.abgefragt_ms = Some(jetzt_ms);
        let m = if neu.verbunden { l.merken_nach.take() } else { None };
        if !neu.verbunden && !neu.verbindet && alt.verbindet {
            // Versuch gescheitert: nicht merken
            l.merken_nach = None;
        }
        if neu.verbunden {
            l.pause_ms = 0;
            l.versuche = 0;
        }
        (alt, m)
    });
    drop(betrieb);
    if neu.verbunden && !alt.verbunden {
        ereignisse.push(Ereignis::Verbunden { ip: neu.ip, rssi: neu.rssi, kanal: neu.kanal, he: neu.he });
    }
    if !neu.verbunden && alt.verbunden {
        ereignisse.push(Ereignis::Getrennt { grund: neu.grund });
    }
    if let Some((s, p)) = merken_nach {
        merken(&s, &p);
    }
    sichern_falls_noetig(t);
    if neu.verbunden || neu.verbindet || !neu.bereit {
        return ereignisse;
    }
    // Nicht verbunden: bekannte Netze probieren?
    let bereit = mit_lage(|l| !l.von_hand_getrennt && jetzt_ms >= l.naechster_versuch_ms && !bekannt_laden(l).is_empty());
    if !bereit {
        return ereignisse;
    }
    let (liste, auftrag_vorher) = mit_lage(|l| (bekannt_laden(l).clone(), l.auftrag));
    let gefunden = suchen().unwrap_or_default();
    // Während der Suche (einige Sekunden) von Hand verbunden oder getrennt?
    // Dann nicht dazwischenfunken.
    let still = mit_lage(|l| l.auftrag != auftrag_vorher || l.von_hand_getrennt || l.zustand.verbindet || l.zustand.verbunden);
    if still {
        return ereignisse;
    }
    // Bekannte Netze in Reichweite, stärkstes zuerst
    let mut kandidaten: Vec<(usize, i32)> = liste
        .iter()
        .enumerate()
        .filter_map(|(i, (s, _))| gefunden.iter().find(|n| n.ssid == *s).map(|n| (i, n.rssi)))
        .collect();
    kandidaten.sort_by(|a, b| b.1.cmp(&a.1));
    let versuche = mit_lage(|l| {
        l.pause_ms = (l.pause_ms * 2).clamp(15_000, 300_000);
        l.naechster_versuch_ms = jetzt_ms + l.pause_ms;
        l.versuche += 1;
        l.versuche
    });
    if kandidaten.is_empty() {
        ereignisse.push(Ereignis::KeinBekanntesNetz);
        return ereignisse;
    }
    // Bei wiederholtem Scheitern reihum die nächsten Kandidaten
    let nr = kandidaten[(versuche as usize - 1) % kandidaten.len()].0;
    let (s, p) = &liste[nr];
    if verbinden(s, p, true).is_ok() {
        ereignisse.push(Ereignis::Versuch { nr });
    }
    ereignisse
}

/// Simulierter Treiber (PC-Tests, Desktop am PC): feste Netzliste,
/// Verbinden gelingt mit dem richtigen Passwort beim nächsten [`wache`].
pub struct Simulation {
    netze: Vec<(Netz, String)>,
    innen: Mutex<SimInnen>,
}

#[derive(Default)]
struct SimInnen {
    verbunden: Option<String>,
    /// Verbindungsauftrag: (SSID, Passwort richtig?).
    auftrag: Option<(String, bool)>,
    grund: u16,
    gespeichert: Vec<(String, String)>,
    suchen_zahl: u32,
    sichern_zahl: u32,
}

impl Simulation {
    /// `netze`: (Netz, Passwort).
    pub fn neu(netze: Vec<(Netz, String)>) -> Simulation {
        Simulation { netze, innen: Mutex::new(SimInnen::default()) }
    }

    /// Mit gespeicherten bekannten Netzen (wie nach einem Neustart).
    pub fn mit_gespeicherten(self, g: Vec<(String, String)>) -> Simulation {
        self.innen.lock().unwrap_or_else(|e| e.into_inner()).gespeichert = g;
        self
    }

    /// Netz verschwindet (Zugangspunkt aus): Verbindung trennen.
    pub fn abbrechen(&self, grund: u16) {
        let mut i = self.innen.lock().unwrap_or_else(|e| e.into_inner());
        i.verbunden = None;
        i.grund = grund;
    }

    fn sperre(&self) -> std::sync::MutexGuard<'_, SimInnen> {
        crate::kern_abschnitt(|| self.innen.lock().unwrap_or_else(|e| e.into_inner()))
    }

    /// Anzahl Suchläufe (Tests).
    pub fn suchen_zahl(&self) -> u32 {
        self.sperre().suchen_zahl
    }

    /// Anzahl Schreibvorgänge der bekannten Netze (Tests).
    pub fn sichern_zahl(&self) -> u32 {
        self.sperre().sichern_zahl
    }
}

impl Treiber for Simulation {
    fn zustand(&self) -> Zustand {
        let mut i = self.sperre();
        if let Some((s, ok)) = i.auftrag.take() {
            if ok {
                i.verbunden = Some(s);
                i.grund = 0;
            } else {
                i.verbunden = None;
                i.grund = if self.netze.iter().any(|(n, _)| n.ssid == s) { 15 } else { 201 };
            }
            // Auftrag erledigt; „verbindet“ endet hier
        }
        let netz = i.verbunden.as_ref().and_then(|s| self.netze.iter().find(|(n, _)| n.ssid == *s));
        Zustand {
            bereit: true,
            verbindet: false,
            verbunden: netz.is_some(),
            ip: if netz.is_some() { [192, 168, 1, 50] } else { [0; 4] },
            rssi: netz.map_or(0, |(n, _)| n.rssi),
            kanal: netz.map_or(0, |(n, _)| n.kanal),
            he: netz.is_some_and(|(n, _)| n.ax),
            grund: i.grund,
            maske: if netz.is_some() { [255, 255, 255, 0] } else { [0; 4] },
            gateway: if netz.is_some() { [192, 168, 1, 1] } else { [0; 4] },
            dns: if netz.is_some() { [192, 168, 1, 1] } else { [0; 4] },
            mac: [0x02, 0, 0, 0x45, 0x53, 0x50],
        }
    }

    fn suchen(&self) -> Result<Vec<Netz>, i32> {
        self.sperre().suchen_zahl += 1;
        Ok(self.netze.iter().map(|(n, _)| n.clone()).collect())
    }

    fn verbinden(&self, ssid: &str, passwort: &str) -> Result<(), i32> {
        let ok = self.netze.iter().any(|(n, p)| n.ssid == ssid && (!n.sicherheit.braucht_passwort() || p == passwort));
        let mut i = self.sperre();
        i.verbunden = None;
        i.auftrag = Some((ssid.to_string(), ok));
        Ok(())
    }

    fn trennen(&self) {
        let mut i = self.sperre();
        i.verbunden = None;
        i.auftrag = None;
        i.grund = 8;
    }

    fn laden(&self) -> Vec<(String, String)> {
        self.sperre().gespeichert.clone()
    }

    fn sichern(&self, netze: &[(String, String)]) -> bool {
        let mut i = self.sperre();
        i.gespeichert = netze.to_vec();
        i.sichern_zahl += 1;
        true
    }
}

// ---- Öffentliche Einstiege: geschützt (Zwangsbeenden, A6)

/// [`suchen_innen`] im geschützten Abschnitt (`BETRIEB` und Treiber).
pub fn suchen() -> Result<Vec<Netz>, i32> {
    crate::geschuetzt(|| suchen_innen())
}

/// [`verbinden_innen`] im geschützten Abschnitt (`BETRIEB` und Treiber).
pub fn verbinden(ssid: &str, passwort: &str, merken: bool) -> Result<(), i32> {
    crate::geschuetzt(|| verbinden_innen(ssid, passwort, merken))
}

/// [`trennen_innen`] im geschützten Abschnitt (`BETRIEB` und Treiber).
pub fn trennen() {
    crate::geschuetzt(|| trennen_innen())
}
