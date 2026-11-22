//! Spielstände und Startziel im NVS-Bereich des Flash (ESP-IDF
//! „Non-Volatile Storage“): verteilt die Schreibzugriffe und übersteht
//! Stromausfälle beim Speichern. Ein Schlüssel je Spiel (siehe
//! `SpielId::nvs_schluessel`), alle im Namensraum „pudding“.

use esp_idf_svc::sys::{self, EspError, esp};
use konsole::start::Speicher;

pub struct Nvs {
    handle: sys::nvs_handle_t,
}

/// NVS-Schlüssel als C-Zeichenkette (höchstens 15 Zeichen).
fn c_schluessel(k: &str) -> [u8; 16] {
    let mut b = [0u8; 16];
    let n = k.len().min(15);
    b[..n].copy_from_slice(&k.as_bytes()[..n]);
    b
}

impl Nvs {
    pub fn open() -> Result<Self, EspError> {
        // SAFETY: einmalige Initialisierung beim Start.
        unsafe {
            if let Err(e) = esp!(sys::nvs_flash_init()) {
                let c = e.code();
                if c == sys::ESP_ERR_NVS_NO_FREE_PAGES as i32 || c == sys::ESP_ERR_NVS_NEW_VERSION_FOUND as i32 {
                    log::warn!("NVS-Bereich wird neu angelegt");
                    esp!(sys::nvs_flash_erase())?;
                    esp!(sys::nvs_flash_init())?;
                } else {
                    return Err(e);
                }
            }
            let mut handle = 0;
            esp!(sys::nvs_open(c"pudding".as_ptr(), sys::nvs_open_mode_t_NVS_READWRITE, &mut handle))?;
            Ok(Self { handle })
        }
    }

    fn commit(&self, r: sys::esp_err_t, was: &str, k: &str) {
        // SAFETY: gültiger Handle.
        let r = esp!(r).and_then(|_| unsafe { esp!(sys::nvs_commit(self.handle)) });
        if let Err(e) = r {
            log::error!("NVS {was} „{k}“ fehlgeschlagen: {e}");
        }
    }
}

impl Speicher for Nvs {
    fn lesen(&self, k: &str) -> Option<Vec<u8>> {
        let key = c_schluessel(k);
        let mut len = 0usize;
        // SAFETY: Abfrage der Länge (Puffer null), dann Lesen in einen
        // passend großen Puffer.
        unsafe {
            esp!(sys::nvs_get_blob(self.handle, key.as_ptr().cast(), core::ptr::null_mut(), &mut len)).ok()?;
            let mut buf = vec![0u8; len];
            esp!(sys::nvs_get_blob(self.handle, key.as_ptr().cast(), buf.as_mut_ptr().cast(), &mut len)).ok()?;
            buf.truncate(len);
            Some(buf)
        }
    }

    fn schreiben(&self, k: &str, daten: &[u8]) {
        let key = c_schluessel(k);
        // SAFETY: Schlüssel nullterminiert, Daten und Länge passen zusammen.
        let r = unsafe { sys::nvs_set_blob(self.handle, key.as_ptr().cast(), daten.as_ptr().cast(), daten.len()) };
        self.commit(r, "Schreiben", k);
    }

    fn loeschen(&self, k: &str) {
        let key = c_schluessel(k);
        // SAFETY: wie oben.
        let r = unsafe { sys::nvs_erase_key(self.handle, key.as_ptr().cast()) };
        if r == sys::ESP_ERR_NVS_NOT_FOUND as i32 {
            return;
        }
        self.commit(r, "Löschen", k);
    }
}
