//! Q2: Text über HTTPS abrufen (Wetter-App). Die Firmware meldet ihren
//! Abrufer an (esp_http_client mit Zertifikatsprüfung, wie das Update); am
//! PC setzen die Tests einen eigenen. Neue Verbindungen nur mit genug
//! internem RAM (M3, wie `netz`).

use std::sync::OnceLock;

/// Ein HTTPS-Abruf: (HTTP-Status, Inhalt) oder verständlicher Fehler.
pub trait Abrufer: Send + Sync {
    fn text(&self, url: &str) -> Result<(i32, String), String>;
}

static ABRUFER: OnceLock<Box<dyn Abrufer>> = OnceLock::new();

/// Abrufer anmelden (Firmware bzw. Test, einmal).
pub fn abrufer_setzen(a: Box<dyn Abrufer>) -> bool {
    crate::kern_abschnitt(|| ABRUFER.set(a).is_ok())
}

/// Text holen. Ohne Abrufer (kein WLAN-Treiber) oder ohne internen RAM
/// für TLS: Fehler in Worten.
pub fn text(url: &str) -> Result<(i32, String), String> {
    let Some(a) = ABRUFER.get() else {
        return Err(crate::sprache::t("No network", "Kein Netzwerk").into());
    };
    let frei = crate::prozess::intern_frei().min(crate::netz::TEST_INTERN_FREI.load(std::sync::atomic::Ordering::Relaxed));
    if frei < crate::netz::NETZ_RESERVE_INTERN {
        return Err(crate::sprache::t("Not enough memory for a connection", "Zu wenig Speicher für eine Verbindung").into());
    }
    a.text(url)
}
