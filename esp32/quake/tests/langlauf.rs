//! Langlauf (nur PC, 32 Bit): Quake startet wie auf dem Board – Stapel
//! 512 KB, PAK stückweise über `Pak::Lesen` – und spielt nacheinander alle
//! Shareware-Level (start, e1m1 … e1m8) mit simulierten Touch-Eingaben
//! (laufen, drehen, schießen, springen, Waffe wechseln), Bildzeiten wie am
//! Board (2/55 s, ab und zu 3/55 und 4/55 s). Dazwischen Spielstand
//! speichern und laden, „map“ und „restart“. Je Level: Hunk/Cache/Zone und
//! der bisher tiefste Stapel des Quake-Threads. Zum Schluss Blick unter
//! Wasser bei „Screen size“ 100/110/120 (Warp-Puffer in R_RenderView_).
//!
//! ```text
//! cargo test --profile sim -q -p quake --target i686-pc-windows-msvc \
//!     --test langlauf -- --ignored --nocapture
//! ```
//!
//! Umgebung: `QUAKE_BILDER_JE_LEVEL` (Standard 3000 ≈ 109 s Spielzeit),
//! `QUAKE_STAPEL_KB` (Standard 512 wie `quake::STAPEL`; ab 4096 wird bis
//! ~4 MB markiert, um den echten Höchststand zu messen), `QUAKE_WARP=0`
//! (Warp-Prüfung auslassen).
#![cfg(target_pointer_width = "32")]

use core::ffi::{c_char, c_int};
use konsole::{Spiel, Wechsel};
use pudding_core::input::Input;
use quake::autopilot::eingabe;
use quake::steuerung::Modus;
use quake::{Pak, Quake, befehl};
use std::sync::atomic::{AtomicUsize, Ordering::SeqCst};
use std::sync::{Mutex, OnceLock};
use std::time::Instant;

// --- Globale Variablen von Quake (nur lesen, solange der Quake-Thread wartet) ----

#[repr(C)]
struct CacheSys {
    size: c_int,
    user: *const u8,
    name: [c_char; 16],
    prev: *const CacheSys,
    next: *const CacheSys,
    lru_prev: *const CacheSys,
    lru_next: *const CacheSys,
}

#[repr(C)]
struct MemBlock {
    size: c_int,
    tag: c_int,
    id: c_int,
    next: *const MemBlock,
    prev: *const MemBlock,
    pad: c_int,
}

#[repr(C)]
struct MemZone {
    size: c_int,
    blocklist: MemBlock,
    rover: *const MemBlock,
}

unsafe extern "C" {
    static hunk_size: c_int;
    static hunk_low_used: c_int;
    static hunk_high_used: c_int;
    static cache_head: CacheSys;
    /// `mainzone` (umbenannt in qnamen.h).
    static q_mainzone: *const MemZone;
    /// server_t: erstes Feld `active`.
    static sv: c_int;
    /// client_static_t: erstes Feld `state` (2 = verbunden).
    static cls: c_int;
    /// client_state_t: `stats` ab Index 7 (nach movemessages und usercmd_t).
    static cl: [c_int; 40];
    static key_dest: c_int;
    static pr_global_struct: *const c_int;
    static pr_strings: *const c_char;
    static sv_player: *mut u8;
    static r_dowarp: c_int;
    /// refdef_t: zuerst vrect (x, y, width, height).
    static r_refdef: [c_int; 4];
}

/// # Safety
/// `p` zeigt auf eine Quake-Variable; der Quake-Thread wartet gerade auf
/// seinen nächsten Auftrag (die Bildschleife steht zwischen zwei `update`).
unsafe fn lies<T: Copy>(p: *const T) -> T {
    // SAFETY: siehe oben.
    unsafe { core::ptr::read_volatile(p) }
}

fn g32(p: *const c_int) -> c_int {
    // SAFETY: Quake-Global, Quake-Thread wartet.
    unsafe { lies(p) }
}

struct Speicher {
    frei_kb: i32,
    tief_kb: i32,
    hoch_kb: i32,
    cache_kb: i32,
    cache_n: i32,
    zone_frei: i32,
    zone_groesster: i32,
}

fn speicher() -> Speicher {
    // SAFETY: Hunk-Zähler, Cache-Liste (cache_system_t) und Zone (memzone_t)
    // wie in zone.c; der Quake-Thread wartet.
    unsafe {
        let (g, lo, hi) = (lies(&raw const hunk_size), lies(&raw const hunk_low_used), lies(&raw const hunk_high_used));
        let (mut cache, mut n) = (0i32, 0i32);
        let kopf = &raw const cache_head;
        let mut c = lies(&raw const (*kopf).next);
        while !core::ptr::eq(c, kopf) && n < 100_000 {
            cache += lies(&raw const (*c).size);
            n += 1;
            c = lies(&raw const (*c).next);
        }
        let (mut zfrei, mut zgr) = (0, 0);
        let z = lies(&raw const q_mainzone);
        if !z.is_null() {
            let ende = &raw const (*z).blocklist;
            let mut b = lies(&raw const (*ende).next);
            let mut k = 0;
            while !core::ptr::eq(b, ende) && k < 100_000 {
                if lies(&raw const (*b).tag) == 0 {
                    let s = lies(&raw const (*b).size);
                    zfrei += s;
                    zgr = zgr.max(s);
                }
                b = lies(&raw const (*b).next);
                k += 1;
            }
        }
        Speicher {
            frei_kb: (g - lo - hi) / 1024,
            tief_kb: lo / 1024,
            hoch_kb: hi / 1024,
            cache_kb: cache / 1024,
            cache_n: n,
            zone_frei: zfrei,
            zone_groesster: zgr,
        }
    }
}

fn kartenname() -> String {
    // SAFETY: globalvars_t: pad[28], self, other, world, time, frametime,
    // force_retouch, mapname (Index 34); Zeichenketten ab pr_strings.
    unsafe {
        let g = lies(&raw const pr_global_struct);
        let s = lies(&raw const pr_strings);
        if g.is_null() || s.is_null() {
            return "?".into();
        }
        let ofs = lies(g.add(34));
        core::ffi::CStr::from_ptr(s.add(ofs as usize)).to_string_lossy().into_owned()
    }
}

fn stat(i: usize) -> i32 {
    // SAFETY: cl.stats[i] (i < 32), Quake-Thread wartet.
    unsafe { lies((&raw const cl).cast::<c_int>().add(7 + i)) }
}

/// Lage der Spielerin (sv_player->v.origin).
fn lage() -> [f32; 3] {
    // SAFETY: edict_t: v ab 96, v.origin ab 40 (progs.h, progdefs.h).
    unsafe {
        let e = lies(&raw const sv_player);
        if e.is_null() { [0.0; 3] } else { lies(e.add(136).cast::<[f32; 3]>()) }
    }
}

// --- PAK wie auf dem Board (stückweise lesen) ---------------------------------------

static PAKDATEN: OnceLock<Vec<u8>> = OnceLock::new();

fn lesen(pos: usize, ziel: &mut [u8]) -> bool {
    let d = PAKDATEN.get().expect("PAK");
    match d.get(pos..pos + ziel.len()) {
        Some(q) => {
            ziel.copy_from_slice(q);
            true
        }
        None => false,
    }
}

fn pak_datei<'a>(pak: &'a [u8], name: &str) -> Option<&'a [u8]> {
    let u = |o: usize| u32::from_le_bytes(pak[o..o + 4].try_into().unwrap()) as usize;
    let (ofs, len) = (u(4), u(8));
    (0..len / 64).map(|i| ofs + i * 64).find_map(|e| {
        let n = &pak[e..e + 56];
        let n = &n[..n.iter().position(|&b| b == 0).unwrap_or(56)];
        (n == name.as_bytes()).then(|| &pak[u(e + 56)..u(e + 56) + u(e + 60)])
    })
}

// --- Thread mit Stapel wie auf dem Board, Stapel vorher markiert ---------------------

static STAPEL_KB: AtomicUsize = AtomicUsize::new(512);
/// Markierter Bereich [unten, oben) des Quake-Stapels.
static MARKE: Mutex<(usize, usize)> = Mutex::new((0, 0));
static FADEN: Mutex<Option<std::thread::JoinHandle<()>>> = Mutex::new(None);
const MUSTER: u8 = 0xA5;

#[inline(never)]
fn markieren<const N: usize>() -> (usize, usize) {
    let mut a = core::mem::MaybeUninit::<[u8; N]>::uninit();
    // SAFETY: schreibt nur in den eigenen (uninitialisierten) Puffer.
    unsafe { core::ptr::write_bytes(a.as_mut_ptr().cast::<u8>(), MUSTER, N) };
    core::hint::black_box(&mut a);
    let p = a.as_ptr() as usize;
    (p, p + N)
}

fn starter(f: Box<dyn FnOnce() + Send + 'static>) {
    let kb = STAPEL_KB.load(SeqCst);
    let h = std::thread::Builder::new()
        .name("quake".into())
        .stack_size(kb * 1024)
        .spawn(move || {
            // Bis auf 64 KB Rest markieren (Windows braucht Schutzseiten).
            let m = if kb >= 4096 { markieren::<{ 4032 * 1024 }>() } else { markieren::<{ 448 * 1024 }>() };
            *MARKE.lock().unwrap() = m;
            f();
        })
        .expect("Quake-Thread");
    *FADEN.lock().unwrap() = Some(h);
}

/// Tiefster bisher beschriebener Punkt des Quake-Stapels, gemessen ab dem
/// oberen Ende der Markierung (KB). `None`: über die Markierung hinaus.
fn stapel_kb() -> Option<usize> {
    let (u, o) = *MARKE.lock().unwrap();
    if u == 0 {
        return Some(0);
    }
    // SAFETY: Stapel des wartenden Quake-Threads; die Seiten sind angelegt.
    let erste = (u..o).find(|&a| unsafe { core::ptr::read_volatile(a as *const u8) } != MUSTER)?;
    if erste == u {
        return None;
    }
    Some((o - erste).div_ceil(1024))
}

// --- Simulierte Spielerin (dieselbe wie im Autopiloten) -------------------------------

/// Bildzeit wie am Board: meist 2 Bildwechsel (27,5 Bilder/s), ab und zu
/// ein verpasster (3) oder zwei (4).
fn bildzeit(n: u32) -> f32 {
    let wechsel = if n % 31 == 0 {
        4.0
    } else if n % 7 == 0 {
        3.0
    } else {
        2.0
    };
    wechsel / 55.0
}

struct Lauf {
    q: Quake,
    gesamt: u32,
    tode: u32,
    lebend: bool,
    weg: f32,
    schuesse: u32,
}

impl Lauf {
    fn bilder(&mut self, n: u32, ein: impl FnMut(u32) -> Input) {
        self.bilder_(n, ein, true);
    }

    fn bilder_(&mut self, n: u32, mut ein: impl FnMut(u32) -> Input, zaehlen: bool) {
        for f in 0..n {
            let (vorher, munition) = (lage(), stat(6) + stat(7));
            let w = self.q.update(&ein(f), bildzeit(self.gesamt));
            assert!(matches!(w, Wechsel::Bleiben), "Quake will beenden (Bild {})", self.gesamt);
            self.gesamt += 1;
            if !zaehlen {
                self.lebend = false;
                continue;
            }
            let leben = stat(0) > 0;
            if self.lebend && !leben {
                self.tode += 1;
            }
            self.lebend = leben;
            let p = lage();
            let d = ((p[0] - vorher[0]).powi(2) + (p[1] - vorher[1]).powi(2) + (p[2] - vorher[2]).powi(2)).sqrt();
            if d < 200.0 {
                self.weg += d;
            }
            if stat(6) + stat(7) < munition {
                self.schuesse += 1;
            }
        }
    }

    fn befehl_und_warten(&mut self, b: &str) {
        befehl(b);
        // Laden und Anmelden (Signon) dauert ein paar Bilder; ohne Eingabe.
        self.bilder_(40, |_| Input::default(), false);
    }

    fn bericht(&self, was: &str, t0: Instant) {
        let s = speicher();
        let st = stapel_kb().map_or("> Markierung".to_string(), |k| format!("{k} KB"));
        println!(
            "[{was:>10}] Karte {:<6} Bilder {:>6}  Hunk frei {:>5} KB (unten {:>5}, oben {:>4}), Cache {:>5} KB/{:>3}, \
             Zone frei {:>5} B (größter {:>5}), Stapel {st}, Leben {:>4}, Tode {}, Weg {:.0}, Schüsse {}, \
             sv.active {}, cls.state {}, key_dest {}, {:.0} s",
            kartenname(),
            self.gesamt,
            s.frei_kb,
            s.tief_kb,
            s.hoch_kb,
            s.cache_kb,
            s.cache_n,
            s.zone_frei,
            s.zone_groesster,
            stat(0),
            self.tode,
            self.weg,
            self.schuesse,
            g32(&raw const sv),
            g32(&raw const cls),
            g32(&raw const key_dest),
            t0.elapsed().as_secs_f32()
        );
    }

    fn pruefe_im_spiel(&self, karte: &str) {
        assert_eq!(g32(&raw const sv), 1, "Server läuft nicht mehr (Host_Error?) in {karte}");
        assert_eq!(g32(&raw const cls), 2, "Client nicht verbunden in {karte}");
        assert_eq!(kartenname(), karte, "falsche Karte");
        assert_eq!(self.q.modus(), Modus::Spiel, "nicht im Spiel in {karte}");
    }
}

#[test]
#[ignore = "Langlauf: braucht pak/pak0.pak, nur mit --ignored"]
fn alle_level_lange_spielen() {
    let pfad = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("pak/pak0.pak");
    let daten = std::fs::read(&pfad).expect("pak/pak0.pak fehlt (pak/holen.py)");
    let groesse = daten.len();
    PAKDATEN.set(daten).unwrap();
    let je_level: u32 = std::env::var("QUAKE_BILDER_JE_LEVEL").ok().and_then(|v| v.parse().ok()).unwrap_or(3000);
    if let Some(kb) = std::env::var("QUAKE_STAPEL_KB").ok().and_then(|v| v.parse().ok()) {
        STAPEL_KB.store(kb, SeqCst);
    }
    println!("Stapel {} KB, {je_level} Bilder je Level (~{:.0} s Spielzeit)", STAPEL_KB.load(SeqCst), je_level as f32 * 2.2 / 55.0);

    let ordner = std::env::temp_dir().join("quake_langlauf");
    let _ = std::fs::remove_dir_all(&ordner);
    std::fs::create_dir_all(ordner.join("id1")).unwrap();
    Quake::virtuelle_zeit();
    let t0 = Instant::now();
    let q = Quake::starten_mit(Pak::Lesen { groesse, lesen }, pfad.to_str().unwrap(), ordner.to_str().unwrap(), starter);
    let mut l = Lauf { q, gesamt: 0, tode: 0, lebend: false, weg: 0.0, schuesse: 0 };
    l.bilder(120, |_| Input::default());
    l.bericht("Vorführung", t0);

    // Neues Spiel wie im Menü („New Game“).
    l.befehl_und_warten("maxplayers 1\nskill 1\nmap start\n");
    l.pruefe_im_spiel("start");
    l.bilder(je_level, eingabe);
    l.bericht("start", t0);

    for n in 1..=8 {
        let karte = format!("e1m{n}");
        // Wie am Levelende: changelevel behält Waffen und Munition.
        l.befehl_und_warten(&format!("changelevel {karte}\n"));
        l.pruefe_im_spiel(&karte);
        l.bericht("geladen", t0);
        l.bilder(je_level / 2, eingabe);
        if n == 3 {
            // Spielstand wie im Menü (s0) speichern, weiterspielen, laden.
            l.befehl_und_warten("save s0\n");
            assert!(ordner.join("id1/s0.sav").exists(), "Spielstand fehlt");
            l.bilder(200, eingabe);
            l.befehl_und_warten("load s0\n");
            l.pruefe_im_spiel(&karte);
            l.bericht("geladen s0", t0);
        }
        l.bilder(je_level - je_level / 2, |f| eingabe(f + 7));
        l.pruefe_im_spiel(&karte);
        l.bericht(&karte, t0);
    }

    // Nochmal von vorn (map statt changelevel) und „restart“ wie nach dem Tod.
    l.befehl_und_warten("map e1m1\n");
    l.pruefe_im_spiel("e1m1");
    l.bilder(je_level / 2, eingabe);
    l.befehl_und_warten("restart\n");
    l.pruefe_im_spiel("e1m1");
    l.bilder(je_level / 2, eingabe);
    l.bericht("e1m1 neu", t0);

    if std::env::var("QUAKE_WARP").as_deref() != Ok("0") {
        warp_pruefen(&mut l, t0);
    }
    println!("Langlauf fertig: {} Bilder, {} Tode, {:.0} s", l.gesamt, l.tode, t0.elapsed().as_secs_f32());
    drop(l);
    if let Some(h) = FADEN.lock().unwrap().take() {
        h.join().unwrap();
    }
}

/// Blick unter Wasser bei „Screen size“ 100, 110 und 120: Unter Wasser
/// rendert Quake die 3D-Sicht erst in den Warp-Puffer von R_RenderView_
/// (r_main.c, WARP_WIDTH × WARP_HEIGHT aus d_iface.h) und verzerrt sie dann
/// ins Bild. Bei 110/120 ist die Sicht 216/240 Zeilen hoch.
fn warp_pruefen(l: &mut Lauf, t0: Instant) {
    let karte = "e1m4";
    l.befehl_und_warten(&format!("map {karte}\ngod\nnoclip\n"));
    l.pruefe_im_spiel(karte);
    let bsp = pak_datei(PAKDATEN.get().unwrap(), &format!("maps/{karte}.bsp")).expect("BSP");
    let i = |o: usize| i32::from_le_bytes(bsp[o..o + 4].try_into().unwrap());
    // Lump 10 = Blätter (dleaf_t, 28 Bytes): Inhalt −3 = Wasser, Mitte der Hülle.
    let (ofs, len) = (i(4 + 10 * 8) as usize, i(4 + 10 * 8 + 4) as usize);
    let mut kandidaten = Vec::new();
    for b in (ofs..ofs + len).step_by(28) {
        if i(b) == -3 {
            let s = |o: usize| i16::from_le_bytes([bsp[b + o], bsp[b + o + 1]]) as f32;
            kandidaten.push([(s(8) + s(14)) / 2.0, (s(10) + s(16)) / 2.0, (s(12) + s(18)) / 2.0]);
        }
    }
    for groesse in [100, 110, 120] {
        befehl(&format!("viewsize {groesse}\n"));
        let mut gefunden = false;
        for p in kandidaten.iter().take(200) {
            // SAFETY: edict_t: v ab 96, v.origin ab 40; Quake-Thread wartet.
            unsafe {
                let e = lies(&raw const sv_player);
                core::ptr::copy_nonoverlapping(p.as_ptr(), e.add(136).cast::<f32>(), 3);
            }
            l.bilder_(4, |_| Input::default(), false);
            if g32(&raw const r_dowarp) != 0 {
                // SAFETY: Quake-Global, Quake-Thread wartet.
                let v = unsafe { lies(&raw const r_refdef) };
                println!("viewsize {groesse}: unter Wasser bei {p:?}, 3D-Sicht y {} Höhe {} -> Zeilen 0..{}", v[1], v[3], v[1] + v[3]);
                l.bilder_(60, |_| Input::default(), false);
                println!("viewsize {groesse}: 60 Bilder unter Wasser überstanden");
                gefunden = true;
                break;
            }
        }
        assert!(gefunden, "kein Wasser gefunden");
        l.bericht("Warp", t0);
    }
}
