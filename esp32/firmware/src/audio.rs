//! Klang auf Kern 1: Synthesizer → I²S (DMA) → ES8311 → Lautsprecher.
//!
//! Der Thread hat hohe Priorität und blockiert nur in `i2s_channel_write`,
//! bis im DMA-Puffer wieder Platz ist – der Ton läuft daher unabhängig von
//! der Bildrate und stotterfrei.

use crate::board::{I2S_BCLK, I2S_DOUT, I2S_MCLK, I2S_WS};
use crate::hw::{spawn_pinned, zeroed};
use esp_idf_svc::sys::{self, EspError, esp};
use pudding_core::audio::{CmdQueue, Klangquelle, SAMPLE_RATE};

/// Klangbefehle Kern 0 → Kern 1 (sperrfrei, `static` im internen RAM).
pub static QUEUE: CmdQueue = CmdQueue::new();

/// Stereo-Frames pro Block (~11,6 ms bei 22 050 Hz).
const BLOCK: usize = 256;

/// I²S-Kanal; der Zeiger gehört danach allein dem Audio-Thread.
struct Tx(sys::i2s_chan_handle_t);
// SAFETY: Der Kanal wird nur noch vom Audio-Thread benutzt.
unsafe impl Send for Tx {}

/// Synthesizer plus PCM-Strom der Programme des Desktops ([`kern::ton`],
/// Musikplayer A5): erst der Synth, dann der Strom dazugemischt. Ohne
/// laufendes Programm ist der Strom leer (Spiele unverändert).
pub struct MitProgrammen(pub pudding_core::audio::Synth);

impl Klangquelle for MitProgrammen {
    fn poll(&mut self, q: &CmdQueue) {
        self.0.poll(q);
    }

    fn render(&mut self, out: &mut [i16]) {
        self.0.render(out);
        kern::ton::dazumischen(out);
    }
}

/// Zeit im Synthesizer (µs, Messung).
pub static RENDER_US: core::sync::atomic::AtomicU32 = core::sync::atomic::AtomicU32::new(0);

pub fn start(synth: Box<dyn Klangquelle>) -> Result<(), EspError> {
    let mut chan: sys::i2s_chan_config_t = zeroed();
    chan.id = sys::i2s_port_t_I2S_NUM_1;
    chan.role = sys::i2s_role_t_I2S_ROLE_MASTER;
    // 6 × 256 Frames ≈ 70 ms Puffer.
    chan.dma_desc_num = 6;
    chan.dma_frame_num = BLOCK as u32;
    let mut tx = core::ptr::null_mut();
    // SAFETY: gültige Konfiguration; nur Sendekanal.
    esp!(unsafe { sys::i2s_new_channel(&chan, &mut tx, core::ptr::null_mut()) })?;

    let mut cfg: sys::i2s_std_config_t = zeroed();
    cfg.clk_cfg.sample_rate_hz = SAMPLE_RATE;
    cfg.clk_cfg.clk_src = sys::soc_periph_i2s_clk_src_t_I2S_CLK_SRC_DEFAULT;
    cfg.clk_cfg.mclk_multiple = sys::i2s_mclk_multiple_t_I2S_MCLK_MULTIPLE_256;
    cfg.clk_cfg.bclk_div = 8;
    // Philips-I²S, 16 Bit, Stereo (der ES8311 nimmt den linken Kanal).
    cfg.slot_cfg.data_bit_width = sys::i2s_data_bit_width_t_I2S_DATA_BIT_WIDTH_16BIT;
    cfg.slot_cfg.slot_bit_width = sys::i2s_slot_bit_width_t_I2S_SLOT_BIT_WIDTH_AUTO;
    cfg.slot_cfg.slot_mode = sys::i2s_slot_mode_t_I2S_SLOT_MODE_STEREO;
    cfg.slot_cfg.slot_mask = sys::i2s_std_slot_mask_t_I2S_STD_SLOT_BOTH;
    cfg.slot_cfg.ws_width = 16;
    cfg.slot_cfg.ws_pol = false;
    cfg.slot_cfg.bit_shift = true;
    cfg.slot_cfg.left_align = true;
    cfg.gpio_cfg.mclk = I2S_MCLK;
    cfg.gpio_cfg.bclk = I2S_BCLK;
    cfg.gpio_cfg.ws = I2S_WS;
    cfg.gpio_cfg.dout = I2S_DOUT;
    cfg.gpio_cfg.din = sys::gpio_num_t_GPIO_NUM_NC;
    // SAFETY: Kanal stammt aus i2s_new_channel.
    esp!(unsafe { sys::i2s_channel_init_std_mode(tx, &cfg) })?;
    esp!(unsafe { sys::i2s_channel_enable(tx) })?;

    let tx = Tx(tx);
    spawn_pinned(c"audio", 1, 20, 6 * 1024, move || run(tx, synth));
    Ok(())
}

fn run(tx: Tx, mut synth: Box<dyn Klangquelle>) {
    let mut pcm = [0i16; BLOCK * 2];
    let mut bytes = [0u8; BLOCK * 4];
    loop {
        synth.poll(&QUEUE);
        let t0 = crate::hw::now_us();
        synth.render(&mut pcm);
        RENDER_US.fetch_add((crate::hw::now_us() - t0) as u32, core::sync::atomic::Ordering::Relaxed);
        // Ein Lautsprecher: beide Kanäle gemischt auf links und rechts.
        for (f, out) in pcm.as_chunks::<2>().0.iter().zip(bytes.as_chunks_mut::<4>().0) {
            let [lo, hi] = (((f[0] as i32 + f[1] as i32) / 2) as i16).to_le_bytes();
            out.copy_from_slice(&[lo, hi, lo, hi]);
        }
        let mut written = 0usize;
        // SAFETY: Puffer gültig während des Aufrufs; blockiert, bis Platz ist.
        unsafe {
            sys::i2s_channel_write(tx.0, bytes.as_ptr().cast(), bytes.len(), &mut written, 1000);
        }
    }
}
