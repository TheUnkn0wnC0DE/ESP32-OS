//! Optionale Akkuanzeige: Spannung über einen Spannungsteiler an einem
//! ADC-Pin (ESP-IDF „ADC oneshot“, kalibriert per Kurvenanpassung – laut
//! Espressif das Verfahren des ESP32-P4).

use crate::board::BatterySense;
use crate::hw::zeroed;
use esp_idf_svc::sys::{self, EspError, esp};
use pudding_core::battery::Smoother;

pub struct Battery {
    unit: sys::adc_oneshot_unit_handle_t,
    chan: sys::adc_channel_t,
    cali: Option<sys::adc_cali_handle_t>,
    divider: f32,
    smooth: Smoother,
}

impl Battery {
    pub fn new(cfg: &BatterySense) -> Result<Self, EspError> {
        let mut unit_id: sys::adc_unit_t = 0;
        let mut chan: sys::adc_channel_t = 0;
        // SAFETY: gültige Zeiger; Handles leben so lange wie das Programm.
        unsafe {
            // Welcher ADC/Kanal zum GPIO gehört, weiß ESP-IDF selbst.
            esp!(sys::adc_oneshot_io_to_channel(cfg.gpio, &mut unit_id, &mut chan))?;
            let mut init: sys::adc_oneshot_unit_init_cfg_t = zeroed();
            init.unit_id = unit_id;
            let mut unit = core::ptr::null_mut();
            esp!(sys::adc_oneshot_new_unit(&init, &mut unit))?;
            let mut cc: sys::adc_oneshot_chan_cfg_t = zeroed();
            cc.atten = sys::adc_atten_t_ADC_ATTEN_DB_12;
            cc.bitwidth = sys::adc_bitwidth_t_ADC_BITWIDTH_DEFAULT;
            esp!(sys::adc_oneshot_config_channel(unit, chan, &cc))?;
            let mut cal: sys::adc_cali_curve_fitting_config_t = zeroed();
            cal.unit_id = unit_id;
            cal.chan = chan;
            cal.atten = sys::adc_atten_t_ADC_ATTEN_DB_12;
            cal.bitwidth = sys::adc_bitwidth_t_ADC_BITWIDTH_DEFAULT;
            let mut cali = core::ptr::null_mut();
            let cali = esp!(sys::adc_cali_create_scheme_curve_fitting(&cal, &mut cali)).ok().map(|_| cali);
            if cali.is_none() {
                log::warn!("Akku: ADC-Kalibrierung nicht verfügbar – keine Akkuanzeige");
            }
            Ok(Self { unit, chan, cali, divider: cfg.divider, smooth: Smoother::default() })
        }
    }

    /// Akkuspannung in mV (16 Messungen gemittelt, dann geglättet); `None`
    /// ohne ADC-Kalibrierung.
    pub fn read_mv(&mut self) -> Option<u32> {
        let cali = self.cali?;
        let mut sum = 0i32;
        for _ in 0..16 {
            let mut raw = 0;
            // SAFETY: Kanal eingerichtet.
            esp!(unsafe { sys::adc_oneshot_read(self.unit, self.chan, &mut raw) }).ok()?;
            sum += raw;
        }
        let mut pin_mv = 0;
        // SAFETY: Kalibrierung gehört zu diesem Kanal.
        esp!(unsafe { sys::adc_cali_raw_to_voltage(cali, sum / 16, &mut pin_mv) }).ok()?;
        Some(self.smooth.push((pin_mv as f32 * self.divider) as u32))
    }
}
