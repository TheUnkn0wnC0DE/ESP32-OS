//! I²C-Bus (ESP-IDF „i2c_master“-Treiber) für Touch und Audio-Codec.

use crate::hw::zeroed;
use esp_idf_svc::sys::{self, EspError, esp};

const TIMEOUT_MS: i32 = 30;

pub struct Bus {
    handle: sys::i2c_master_bus_handle_t,
}

impl Bus {
    pub fn new(port: i32, sda: i32, scl: i32) -> Result<Self, EspError> {
        let mut cfg: sys::i2c_master_bus_config_t = zeroed();
        cfg.i2c_port = port;
        cfg.sda_io_num = sda;
        cfg.scl_io_num = scl;
        cfg.__bindgen_anon_1.clk_source = sys::soc_periph_i2c_clk_src_t_I2C_CLK_SRC_DEFAULT;
        cfg.glitch_ignore_cnt = 7;
        cfg.flags.set_enable_internal_pullup(1);
        let mut handle = core::ptr::null_mut();
        // SAFETY: gültige Konfiguration; der Bus lebt so lange wie das Programm.
        esp!(unsafe { sys::i2c_new_master_bus(&cfg, &mut handle) })?;
        Ok(Self { handle })
    }

    /// Für ESP-IDF-Treiber, die den Bus selbst benutzen (Touch).
    pub fn handle(&self) -> sys::i2c_master_bus_handle_t {
        self.handle
    }

    /// Antwortet ein Baustein unter `addr`?
    pub fn probe(&self, addr: u8) -> bool {
        // SAFETY: Bus ist eingerichtet.
        esp!(unsafe { sys::i2c_master_probe(self.handle, addr as u16, TIMEOUT_MS) }).is_ok()
    }

    pub fn device(&self, addr: u8, hz: u32) -> Result<Device, EspError> {
        let mut cfg: sys::i2c_device_config_t = zeroed();
        cfg.dev_addr_length = sys::i2c_addr_bit_len_t_I2C_ADDR_BIT_LEN_7;
        cfg.device_address = addr as u16;
        cfg.scl_speed_hz = hz;
        let mut handle = core::ptr::null_mut();
        // SAFETY: wie oben.
        esp!(unsafe { sys::i2c_master_bus_add_device(self.handle, &cfg, &mut handle) })?;
        Ok(Device { handle })
    }
}

pub struct Device {
    handle: sys::i2c_master_dev_handle_t,
}

impl Device {
    pub fn write(&self, data: &[u8]) -> bool {
        // SAFETY: Puffer gültig für die Dauer des (blockierenden) Aufrufs.
        esp!(unsafe { sys::i2c_master_transmit(self.handle, data.as_ptr(), data.len(), TIMEOUT_MS) }).is_ok()
    }

    pub fn write_read(&self, w: &[u8], r: &mut [u8]) -> bool {
        // SAFETY: wie oben.
        esp!(unsafe {
            sys::i2c_master_transmit_receive(self.handle, w.as_ptr(), w.len(), r.as_mut_ptr(), r.len(), TIMEOUT_MS)
        })
        .is_ok()
    }
}
