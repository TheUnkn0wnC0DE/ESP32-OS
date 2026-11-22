//! Pudding-Panik – ein lustiges 3D-Jump'n'Run auf der `spatz`-Engine.
//!
//! Ein Vanillepudding muss durch eine Dessertwelt in den Kühlschrank
//! flüchten, bevor er in der Sonne schmilzt.

use pudding::{alloc_count, bench, game, level, verify};
use std::process::ExitCode;

/// Zählt Allokationen (Nachweis: keine Heap-Allokationen pro Frame).
#[global_allocator]
static GLOBAL: alloc_count::Counting = alloc_count::Counting;

const HELP: &str = "\
PUDDING-PANIK  –  3D-Jump'n'Run auf der spatz-Engine (Software-Rendering)

Aufruf:  pudding [Optionen]

Spiel:
  --res BxH          interne Auflösung (Standard 480x270, z. B. 320x240)
  --scale N          Fenster-Vergrößerung (Standard: passend zum Bildschirm)
  --novsync          ohne VSync (zeigt die maximale Bildrate)
  --demo             der Autopilot spielt (TAB schaltet auch im Spiel um)
  --level DATEI      eigenes Level laden (Textformat, siehe levels/level1.txt)
  --nohud            Performance-Overlay beim Start ausblenden

Kopflos (öffnet KEIN Fenster):
  --bench [BILDER]   Benchmark über mehrere Auflösungen (Standard 9000 Bilder max.)
  --screenshots DIR  PNG-Screenshots des Autopilot-Laufs nach DIR
  --gif DATEI [SEK]  Wackel-Clip als animiertes GIF aufnehmen (Standard 18 s)
  --trace           Autopilot-Durchlauf ohne Grafik protokollieren (Level-Test)
  --profile          Zeitprofil eines Frames nach Phasen (mit --res kombinierbar)
  --perf [N]         N komplette Messläufe (Standard 5), Median pro Phase
  --pin CPU          Benchmark-Thread an einen logischen Prozessor binden
  --baseline         nur Basis-Pfade (SSE2) nutzen, AVX2-Varianten abschalten
  --golden-save DIR  bitgenaue Referenz (Frame- und Zustands-Hashes, PNGs) anlegen
  --golden-check DIR aktuelle Version bitgenau gegen die Referenz prüfen

Tasten: WASD/Pfeile laufen, Leertaste springen, Q/E Kamera, P Pause,
        R zum Checkpoint, H HUD, TAB Autopilot, F1 Perf-Overlay, F2 VSync,
        F11/Alt+Enter Vollbild, ESC Ende. Xbox-Controller wird unterstützt.
";

struct Options {
    res: (usize, usize),
    scale: Option<u32>,
    vsync: bool,
    demo: bool,
    level: Option<String>,
    perf: bool,
    bench: Option<usize>,
    screenshots: Option<String>,
    gif: Option<(String, f32)>,
    trace: bool,
    profile: bool,
    perf_runs: Option<usize>,
    golden_save: Option<String>,
    golden_check: Option<String>,
}

fn parse_args() -> Result<Options, String> {
    let mut o = Options {
        res: (480, 270),
        scale: None,
        vsync: true,
        demo: false,
        level: None,
        perf: true,
        bench: None,
        screenshots: None,
        gif: None,
        trace: false,
        profile: false,
        perf_runs: None,
        golden_save: None,
        golden_check: None,
    };
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut i = 0;
    while i < args.len() {
        let a = args[i].as_str();
        let mut val = || {
            i += 1;
            args.get(i).cloned().ok_or_else(|| format!("{a} braucht einen Wert"))
        };
        match a {
            "-h" | "--help" | "/?" => return Err(String::new()),
            "--res" => {
                let v = val()?;
                let (w, h) = v.split_once(['x', 'X']).ok_or("Format: --res 480x270")?;
                let w: usize = w.parse().map_err(|_| "ungültige Breite")?;
                let h: usize = h.parse().map_err(|_| "ungültige Höhe")?;
                if !(16..=4096).contains(&w) || !(16..=4096).contains(&h) {
                    return Err("Auflösung muss zwischen 16 und 4096 liegen".into());
                }
                o.res = (w, h);
            }
            "--scale" => o.scale = Some(val()?.parse().map_err(|_| "ungültiger Faktor")?),
            "--novsync" => o.vsync = false,
            "--demo" => o.demo = true,
            "--nohud" => o.perf = false,
            "--level" => o.level = Some(val()?),
            "--screenshots" => o.screenshots = Some(val()?),
            "--gif" => {
                let file = val()?;
                let secs = args.get(i + 1).and_then(|s| s.parse::<f32>().ok());
                if secs.is_some() {
                    i += 1;
                }
                o.gif = Some((file, secs.unwrap_or(18.0).clamp(0.5, 120.0)));
            }
            "--trace" => o.trace = true,
            "--profile" => o.profile = true,
            "--pin" => {
                let cpu: usize = val()?.parse().map_err(|_| "ungültige CPU-Nummer")?;
                #[cfg(windows)]
                if !spatz::platform::win32::pin_current_thread(cpu) {
                    return Err(format!("Thread konnte nicht an CPU {cpu} gebunden werden"));
                }
                #[cfg(not(windows))]
                let _ = cpu;
            }
            "--baseline" => spatz::cpu::force_baseline(true),
            "--golden-save" => o.golden_save = Some(val()?),
            "--golden-check" => o.golden_check = Some(val()?),
            "--perf" => {
                let n = args.get(i + 1).and_then(|s| s.parse::<usize>().ok());
                if n.is_some() {
                    i += 1;
                }
                o.perf_runs = Some(n.unwrap_or(5).max(1));
            }
            "--bench" => {
                let n = args.get(i + 1).and_then(|s| s.parse::<usize>().ok());
                if n.is_some() {
                    i += 1;
                }
                o.bench = Some(n.unwrap_or(9000));
            }
            other => return Err(format!("Unbekannte Option: {other}")),
        }
        i += 1;
    }
    Ok(o)
}

fn main() -> ExitCode {
    let opts = match parse_args() {
        Ok(o) => o,
        Err(e) => {
            if !e.is_empty() {
                eprintln!("Fehler: {e}\n");
            }
            eprint!("{HELP}");
            return if e.is_empty() { ExitCode::SUCCESS } else { ExitCode::from(2) };
        }
    };
    let text = match &opts.level {
        Some(path) => match std::fs::read_to_string(path) {
            Ok(t) => t,
            Err(e) => {
                eprintln!("Level '{path}' nicht lesbar: {e}");
                return ExitCode::FAILURE;
            }
        },
        None => level::BUILTIN.to_string(),
    };
    let def = match level::parse(&text) {
        Ok(d) => d,
        Err(e) => {
            eprintln!("Levelfehler: {e}");
            return ExitCode::FAILURE;
        }
    };

    if let Some(dir) = &opts.golden_save {
        return match verify::golden_save(&def, std::path::Path::new(dir)) {
            Ok(()) => ExitCode::SUCCESS,
            Err(e) => {
                eprintln!("Referenz konnte nicht gespeichert werden: {e}");
                ExitCode::FAILURE
            }
        };
    }
    if let Some(dir) = &opts.golden_check {
        return match verify::golden_check(&def, std::path::Path::new(dir)) {
            Ok(true) => ExitCode::SUCCESS,
            Ok(false) => ExitCode::FAILURE,
            Err(e) => {
                eprintln!("Referenz nicht lesbar: {e}");
                ExitCode::FAILURE
            }
        };
    }
    if let Some(runs) = opts.perf_runs {
        let mut res = vec![(480, 270), (320, 240)];
        if !res.contains(&opts.res) {
            res.insert(0, opts.res);
        }
        verify::perf(&def, runs, &res);
        return ExitCode::SUCCESS;
    }
    if opts.trace {
        return if bench::trace(&def, true).is_some() { ExitCode::SUCCESS } else { ExitCode::FAILURE };
    }
    if opts.profile {
        bench::profile(&def, opts.res.0, opts.res.1);
        return ExitCode::SUCCESS;
    }
    if let Some(frames) = opts.bench {
        let mut res = vec![(320, 240), (480, 270), (960, 540), (1920, 1080)];
        if !res.contains(&opts.res) {
            res.insert(0, opts.res);
        }
        bench::run(&def, frames, &res);
        return ExitCode::SUCCESS;
    }
    if let Some(dir) = &opts.screenshots {
        return match bench::screenshots(&def, std::path::Path::new(dir), opts.res.0, opts.res.1) {
            Ok(files) => {
                for f in files {
                    println!("{}", f.display());
                }
                ExitCode::SUCCESS
            }
            Err(e) => {
                eprintln!("Screenshots fehlgeschlagen: {e}");
                ExitCode::FAILURE
            }
        };
    }
    if let Some((file, secs)) = &opts.gif {
        // Endet der Name nicht auf .gif, entstehen Einzelbilder (PNG) im Ordner.
        if !file.to_ascii_lowercase().ends_with(".gif") {
            let r = bench::clip_pngs(&def, std::path::Path::new(file), opts.res.0, opts.res.1, *secs, 2);
            return if r.is_ok() { ExitCode::SUCCESS } else { ExitCode::FAILURE };
        }
        return match bench::record_gif(&def, std::path::Path::new(file), opts.res.0, opts.res.1, *secs) {
            Ok((frames, bytes)) => {
                println!("{file}: {frames} Bilder, {:.1} MB", bytes as f64 / 1e6);
                ExitCode::SUCCESS
            }
            Err(e) => {
                eprintln!("GIF fehlgeschlagen: {e}");
                ExitCode::FAILURE
            }
        };
    }
    run_window(&opts, &def)
}

#[cfg(windows)]
fn run_window(opts: &Options, def: &level::LevelDef) -> ExitCode {
    use spatz::app::{AppConfig, run};
    use spatz::platform::win32::{Win32Window, detach_own_console};
    let (w, h) = opts.res;
    let mut window = match Win32Window::new("Pudding-Panik", w, h, opts.scale) {
        Ok(win) => win,
        Err(e) => {
            eprintln!("Fenster konnte nicht geöffnet werden: {e}");
            return ExitCode::FAILURE;
        }
    };
    // Erst nach erfolgreichem Fensterstart: Fehler oben bleiben so sichtbar.
    detach_own_console();
    let mut game = game::PuddingGame::new(def, opts.demo);
    let cfg = AppConfig {
        title: "Pudding-Panik".into(),
        width: w,
        height: h,
        sim_hz: 120.0,
        vsync: opts.vsync,
        show_perf: opts.perf,
        fps_cap: 0,
    };
    run(&mut window, &mut game, &cfg);
    ExitCode::SUCCESS
}

#[cfg(not(windows))]
fn run_window(_opts: &Options, _def: &level::LevelDef) -> ExitCode {
    eprintln!("Auf diesem System gibt es noch kein Fenster-Backend (nur Windows).");
    eprintln!("Kopflos geht: --bench oder --screenshots DIR");
    ExitCode::FAILURE
}
