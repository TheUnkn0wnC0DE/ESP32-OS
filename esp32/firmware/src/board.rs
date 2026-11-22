//! Pinbelegung des Waveshare ESP32-P4-WIFI6-Touch-LCD-5 – nur Werte aus der
//! Dokumentation des Herstellers:
//!
//! * Waveshare-Wiki, Tabelle der 40-Pin-Leiste
//!   (<https://docs.waveshare.com/ESP32-P4-WIFI6-Touch-LCD-5>)
//! * Waveshare-Repository `ESP32-P4-WIFI6-Touch-LCD-5`, `docs/IO.md`
//! * Board-Support-Paket `waveshare/esp32_p4_wifi6_touch_lcd_5` 1.0.4
//!   (`include/bsp/*.h`) und Panel-Treiber `waveshare/esp_lcd_hx8394` 2.1
//! * ADC-Kanäle: ESP-IDF `soc/esp32p4/include/soc/adc_channel.h`
//!
//! | Funktion                 | GPIO / Wert                                   |
//! |--------------------------|-----------------------------------------------|
//! | I²C SDA / SCL            | 7 / 8 (Touch GT911, Codec ES8311, ES7210)      |
//! | I²S MCLK / BCLK / LRCK   | 13 / 12 / 10                                  |
//! | I²S DOUT (zum Codec)     | 9                                             |
//! | Verstärker (PA)          | 53 (high = an)                                |
//! | LCD Reset                | 27 (im BSP: `reset_active_high = 1`)          |
//! | LCD Hintergrundlicht     | 26 (PWM 5 kHz, 10 Bit)                        |
//! | MIPI-DSI                 | 2 Lanes à 700 Mbit/s, PHY-Spannung LDO 3 2,5 V |
//! | Touch Reset / Interrupt  | im BSP nicht benutzt (Schaltplan laut `IO.md`: 23 / 2, Interrupt nur über einen optionalen Widerstand) |
//!
//! ## 40-Pin-Leiste (Auszug aus der Waveshare-Tabelle)
//!
//! Die Leiste ist **nicht** wie beim Raspberry Pi belegt (Pin 1 = 5 V,
//! Pin 2 = 3V3). Wo Pin 1 sitzt, sagt die Tabelle nicht – laut Waveshare vor
//! dem Verdrahten am Platinenaufdruck prüfen.
//!
//! | Leisten-Pin | Signal | hier benutzt für |
//! |-------------|--------|------------------|
//! | 13          | GND    | Akkuanzeige (GND) |
//! | 15          | GPIO21 | Akkuanzeige (ADC1 Kanal 5) |
//! | 30          | GPIO35 | BOOT? (nur Anzeige in `hwtest`) |
//! | 32          | GPIO49 | Knopf Vorne      |
//! | 33          | GND    | Knöpfe (GND)     |
//! | 34          | GPIO50 | Knopf Hinten     |
//! | 36          | GPIO51 | Knopf Links      |
//! | 38          | GPIO52 | Knopf Rechts     |
//! | 39          | GPIO48 | Knopf Springen   |
//! | 40          | GND    | Knöpfe (GND)     |
//!
//! **Knöpfe:** optional, standardmäßig aus ([`BUTTONS_ENABLED`]). Die Konsole
//! ist komplett per Touch bedienbar. Die fünf Knöpfe liegen auf
//! **GPIO 48, 49, 50, 51, 52** (je zwischen Pin und GND, interner Pull-up).
//! Gemieden: 34–38 (Boot-Strapping des ESP32-P4), 24/25 (USB), 2 (evtl.
//! Touch-Interrupt), 7/8 (I²C), 21 (Akkuanzeige).
//!
//! **BOOT-Knopf:** Die Waveshare-Unterlagen nennen seinen GPIO nicht. Er ist
//! daher **nicht** belegt (Pause per Touch oben links). `hwtest` zeigt den
//! Pegel von GPIO 35 (Boot-Strapping-Pin des ESP32-P4): Wechselt er beim
//! Drücken von BOOT, kann [`BTN_MENU`] auf `Some(35)` gesetzt werden.
//!
//! **Akku:** Die Platine lädt den Akku selbst, misst die Spannung aber nicht
//! an einem GPIO. Optional: eigener Spannungsteiler an GPIO 21 ([`BATTERY`]).

/// I²C-Bus 1 (wie im Board-Support-Paket, 400 kHz).
pub const I2C_PORT: i32 = 1;
pub const I2C_SDA: i32 = 7;
pub const I2C_SCL: i32 = 8;
pub const I2C_HZ: u32 = 400_000;

/// ES8311 (`ES8311_CODEC_DEFAULT_ADDR`, 7 Bit).
pub const ADDR_CODEC: u8 = 0x18;
/// GT911: Standard- und Ersatzadresse (wie im Board-Support-Paket geprüft).
pub const ADDR_TOUCH: [u8; 2] = [0x5D, 0x14];

/// Touch abfragen: alle 4 ms (250-mal pro Sekunde, eigener Thread). Der GT911
/// liefert selbst neue Daten etwa alle 10 ms; so geht keine Messung verloren
/// und kurzes Antippen zwischen zwei Bildern wird trotzdem erkannt.
pub const TOUCH_POLL_MS: u32 = 4;
/// Touch-Interrupt: laut `IO.md` an GPIO 2, aber nur über einen optionalen
/// Widerstand – ob er bestückt ist, ist nicht dokumentiert. Daher aus; der
/// Treiber fragt dann im Takt [`TOUCH_POLL_MS`] ab. `hwtest` zählt, ob GPIO 2
/// beim Berühren wechselt; nur dann hier `Some(2)` eintragen.
pub const TOUCH_INT: Option<i32> = None;
/// Wird von `hwtest` beobachtet (siehe [`TOUCH_INT`]).
pub const TOUCH_INT_CANDIDATE: i32 = 2;

/// I²S zum ES8311.
pub const I2S_MCLK: i32 = 13;
pub const I2S_BCLK: i32 = 12;
pub const I2S_WS: i32 = 10;
pub const I2S_DOUT: i32 = 9;
pub const PA_EN: i32 = 53;

/// Display: HX8394, 720×1280 hochkant, MIPI-DSI.
pub const LCD_W: usize = 720;
pub const LCD_H: usize = 1280;
pub const LCD_RST: i32 = 27;
pub const LCD_BACKLIGHT: i32 = 26;
pub const DSI_LANES: u8 = 2;
pub const DSI_LANE_MBPS: u32 = 700;
pub const DSI_PHY_LDO_CHAN: i32 = 3;
pub const DSI_PHY_LDO_MV: i32 = 2500;
/// Zeitverhalten aus `HX8394_720_1280_PANEL_30HZ_DPI_CONFIG` des
/// Waveshare-Treibers: 58 MHz Pixeltakt bei (720+20+20+40) × (1280+4+10+24)
/// = 800 × 1318 Takten → rechnerisch 55,0 Bilder pro Sekunde.
pub const DPI_CLOCK_MHZ: f32 = 58.0;
pub const HSYNC_PULSE: u32 = 20;
pub const HSYNC_BACK: u32 = 20;
pub const HSYNC_FRONT: u32 = 40;
pub const VSYNC_PULSE: u32 = 4;
pub const VSYNC_BACK: u32 = 10;
pub const VSYNC_FRONT: u32 = 24;
/// Rechnerische Bildwiederholrate (≈ 55,0 Hz); die Firmware misst nach.
pub const PANEL_HZ: f32 = DPI_CLOCK_MHZ * 1.0e6
    / (((LCD_W as u32 + HSYNC_PULSE + HSYNC_BACK + HSYNC_FRONT)
        * (LCD_H as u32 + VSYNC_PULSE + VSYNC_BACK + VSYNC_FRONT)) as f32);
/// Zeitbudget pro Bild bei dieser Rate (≈ 18,2 ms).
pub const FRAME_BUDGET_MS: f32 = 1000.0 / PANEL_HZ;

/// Knöpfe an der Leiste – **aus**, solange nur per Touch gespielt wird.
/// Nach dem Anlöten auf `true` setzen (Pins siehe unten und oben).
pub const BUTTONS_ENABLED: bool = false;
/// Knöpfe (aktiv low, interner Pull-up): GPIO 48–52.
pub const BTN_UP: i32 = 49;
pub const BTN_DOWN: i32 = 50;
pub const BTN_LEFT: i32 = 51;
pub const BTN_RIGHT: i32 = 52;
pub const BTN_JUMP: i32 = 48;
/// BOOT-Knopf als Pause/Zurück – erst eintragen, wenn `hwtest` bestätigt
/// hat, an welchem GPIO er hängt (siehe oben). Wirkt auch ohne
/// [`BUTTONS_ENABLED`].
pub const BTN_MENU: Option<i32> = None;
/// Wird von `hwtest` angezeigt, um den BOOT-Knopf zu finden.
pub const BOOT_CANDIDATE: i32 = 35;

/// Akkuspannung über einen Spannungsteiler an einem ADC-Pin.
#[derive(Clone, Copy, Debug)]
pub struct BatterySense {
    pub gpio: i32,
    /// Akkuspannung / Pinspannung, z. B. (200 kΩ + 100 kΩ) / 100 kΩ = 3.
    pub divider: f32,
}

/// Vorschlag (siehe README, „Akkuanzeige“): GPIO 21 = Leisten-Pin 15,
/// ADC1 Kanal 5; Akku+ → 200 kΩ → Pin 15 → 100 kΩ → GND (Pin 13),
/// 100 nF parallel zu den 100 kΩ. Bei 4,2 V liegen am Pin 1,4 V.
pub const BATTERY_SUGGESTED: BatterySense = BatterySense { gpio: 21, divider: 3.0 };

/// Akkuanzeige – **aus**, bis der Spannungsteiler eingelötet ist. Danach:
/// `Some(BATTERY_SUGGESTED)` (und `divider` ggf. mit einem Multimeter
/// nachjustieren: 3,0 × gemessene Akkuspannung ÷ angezeigte Spannung).
pub const BATTERY: Option<BatterySense> = None;
