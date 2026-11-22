//! Programmplätze (B0, neue Aufteilung): ESP32-OS liegt in zwei OTA-Plätzen
//! „esp32os_a“/„esp32os_b“ (Updates über WLAN, Zurückspringen auf die alte
//! Version, wenn die neue nicht startet), Doom und Quake in eigenen Plätzen
//! „doomapp“/„quakeapp“ – so belegen sie keinen internen RAM in den anderen.
//! Ein Spiel startet in seinem Programm; fehlt es im laufenden, wird die
//! Startpartition umgestellt und neu gestartet.
//!
//! Rollback: Der Bootloader ist mit `CONFIG_BOOTLOADER_APP_ROLLBACK_ENABLE`
//! gebaut. Jedes Programm meldet sich nach dem ersten gezeichneten Bild als
//! gültig ([`als_gueltig_markieren`]); stürzt ein frisch gesetztes Programm
//! vorher ab, startet der Bootloader wieder das vorige.

use esp_idf_svc::sys::{self, esp};
use konsole::SpielId;
use konsole::start::Speicher;

const OS: [&core::ffi::CStr; 2] = [c"esp32os_a", c"esp32os_b"];
/// NVS-Schlüssel: zuletzt gestarteter ESP32-OS-Platz (0 = a, 1 = b).
const OS_SCHLUESSEL: &str = "os_platz";

fn finden(name: &core::ffi::CStr) -> *const sys::esp_partition_t {
    // SAFETY: Suche in der (statischen) Partitionstabelle.
    unsafe { sys::esp_partition_find_first(sys::esp_partition_type_t_ESP_PARTITION_TYPE_APP, sys::esp_partition_subtype_t_ESP_PARTITION_SUBTYPE_ANY, name.as_ptr()) }
}

/// Enthält der Platz ein Programm, das nicht als ungültig markiert ist?
fn brauchbar(p: *const sys::esp_partition_t) -> bool {
    if p.is_null() {
        return false;
    }
    // SAFETY: gültige Partition; liest Kopf bzw. otadata.
    unsafe {
        let mut d: sys::esp_app_desc_t = core::mem::zeroed();
        if sys::esp_ota_get_partition_description(p, &mut d) != 0 {
            return false;
        }
        let mut z: sys::esp_ota_img_states_t = 0;
        // otadata über mmap: nur unter der Flash-Sperre
        let ok = crate::flash_sperre::gesperrt(|| sys::esp_ota_get_state_partition(p, &mut z));
        ok != 0 || !(z == sys::esp_ota_img_states_t_ESP_OTA_IMG_INVALID || z == sys::esp_ota_img_states_t_ESP_OTA_IMG_ABORTED)
    }
}

/// Der ESP32-OS-Platz, in den Doom/Quake zurückkehren.
pub fn os_platz() -> &'static core::ffi::CStr {
    let i = crate::store::Nvs::open().ok().and_then(|n| n.lesen(OS_SCHLUESSEL)).and_then(|b| b.first().copied()).map_or(0, |b| usize::from(b & 1));
    if brauchbar(finden(OS[i])) { OS[i] } else { OS[1 - i] }
}

/// Partition des Programms, das `id` enthält.
pub fn partition_fuer(id: SpielId) -> &'static core::ffi::CStr {
    match id {
        SpielId::Doom => c"doomapp",
        SpielId::Quake => c"quakeapp",
        SpielId::Konsole | SpielId::Pudding | SpielId::Wuerfelwelt => os_platz(),
    }
}

/// Ziel eines Updates (U1): der ESP32-OS-Platz, der gerade nicht läuft.
pub fn update_platz() -> &'static core::ffi::CStr {
    if laufend() == "esp32os_b" { OS[0] } else { OS[1] }
}

/// Name der laufenden App-Partition.
pub fn laufend() -> &'static str {
    // SAFETY: liefert einen Zeiger in die (statische) Partitionstabelle.
    unsafe {
        let p = sys::esp_ota_get_running_partition();
        if p.is_null() {
            return "?";
        }
        core::ffi::CStr::from_ptr((*p).label.as_ptr()).to_str().unwrap_or("?")
    }
}

/// Läuft `id` in diesem Programm?
pub fn hier(id: SpielId) -> bool {
    match id {
        SpielId::Konsole | SpielId::Pudding | SpielId::Wuerfelwelt => laufend().starts_with("esp32os"),
        _ => partition_fuer(id).to_str().ok() == Some(laufend()),
    }
}

/// Läuft ESP32-OS: seinen Platz merken (Rückkehr aus Doom/Quake, nach
/// einem Update in den anderen Platz).
pub fn os_platz_merken() {
    let l = laufend();
    if let Some(i) = OS.iter().position(|o| o.to_str().ok() == Some(l))
        && let Ok(n) = crate::store::Nvs::open()
        && n.lesen(OS_SCHLUESSEL).as_deref() != Some(&[i as u8])
    {
        n.schreiben(OS_SCHLUESSEL, &[i as u8]);
    }
}

/// Beim nächsten Start das Programm mit `id` laden. `false`, wenn die
/// Partition fehlt oder kein gültiges Programm enthält.
pub fn umschalten(id: SpielId) -> bool {
    let name = partition_fuer(id);
    let p = finden(name);
    if p.is_null() {
        log::error!("Programmplatz {name:?} fehlt (partitions.csv flashen)");
        return false;
    }
    // SAFETY: gültige Partition; Startpartition setzen liest (mmap) und
    // schreibt otadata – unter der Flash-Sperre.
    match crate::flash_sperre::gesperrt(|| unsafe { esp!(sys::esp_ota_set_boot_partition(p)) }) {
        Ok(()) => true,
        Err(e) => {
            log::error!("Programmplatz {name:?}: {e} (Programm aufgespielt? bauen_flashen.ps1)");
            false
        }
    }
}

/// Laufendes Programm als gültig melden (Rollback abbrechen). Meldet, ob
/// vorher ein anderes Programm zurückgesetzt wurde.
pub fn als_gueltig_markieren() {
    // SAFETY: einfache Abfragen/Markierung im otadata (mmap) – unter der
    // Flash-Sperre.
    crate::flash_sperre::gesperrt(|| unsafe {
        let mut z: sys::esp_ota_img_states_t = 0;
        let lauf = sys::esp_ota_get_running_partition();
        let ausstehend = sys::esp_ota_get_state_partition(lauf, &mut z) == 0 && z == sys::esp_ota_img_states_t_ESP_OTA_IMG_PENDING_VERIFY;
        let r = sys::esp_ota_mark_app_valid_cancel_rollback();
        log::info!("Programm „{}“ als gültig markiert ({}, Code {r})", laufend(), if ausstehend { "war in Prüfung" } else { "schon gültig" });
        let ungueltig = sys::esp_ota_get_last_invalid_partition();
        if !ungueltig.is_null() {
            let n = core::ffi::CStr::from_ptr((*ungueltig).label.as_ptr()).to_str().unwrap_or("?");
            log::warn!("Rollback: „{n}“ ist als ungültig markiert (startete nicht) – läuft wieder „{}“", laufend());
        }
    });
}

/// Entwicklungs-Build `ESP32OS_OTA_TEST=1` (B0-Nachweis Rollback): einmal
/// (NVS-Merker) auf den anderen ESP32-OS-Platz umschalten und neu starten.
/// Liegt dort ein Programm, das sich nicht als gültig meldet, springt der
/// Bootloader zurück; [`als_gueltig_markieren`] meldet das im Log.
pub fn ota_test() {
    let Ok(n) = crate::store::Nvs::open() else { return };
    if n.lesen("ota_test").is_some() {
        log::warn!("OTA-Test: schon durchgeführt");
        return;
    }
    n.schreiben("ota_test", &[1]);
    let l = laufend();
    let ziel = if l == "esp32os_a" { OS[1] } else { OS[0] };
    let p = finden(ziel);
    // SAFETY: gültige Partition; Startpartition setzen schreibt otadata.
    match crate::flash_sperre::gesperrt(|| unsafe { esp!(sys::esp_ota_set_boot_partition(p)) }) {
        Ok(()) => {
            log::warn!("OTA-Test: Startpartition {ziel:?} gesetzt – Neustart");
            // SAFETY: Neustart.
            unsafe { sys::esp_restart() };
        }
        Err(e) => log::error!("OTA-Test: {ziel:?}: {e}"),
    }
}
