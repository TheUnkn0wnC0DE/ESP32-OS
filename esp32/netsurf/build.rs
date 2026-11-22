//! Übersetzt NetSurf (Upstream 3.11, vorbereitet von
//! `werkzeuge/quellen_holen.py` unter `C:\ns\3.11`, anders mit
//! `NETSURF_QUELLEN`) mit QuickJS-ng und der ESP32-OS-Anbindung.
//!
//! Die Quellgruppen (je eine Bibliothek mit eigenen Pfaden und Defines)
//! stehen einmal in [`gruppen`]: Am PC übersetzt dieses Skript sie mit
//! clang-cl; für das Board schreibt es dieselben Gruppen als CMake-Liste
//! `<Quellen>/erzeugt/netsurf_quellen.cmake`, die die ESP-IDF-Komponente
//! `firmware/komponenten/netsurf` einliest – beide Bauten übersetzen so
//! genau dieselben Dateien. Quelllisten der Bibliotheken liest es wie das
//! Upstream-Buildsystem aus den `DIR_SOURCES` der Makefiles unter `src/`.

use std::path::{Path, PathBuf};

fn quellen() -> PathBuf {
    println!("cargo:rerun-if-env-changed=NETSURF_QUELLEN");
    let p = PathBuf::from(std::env::var("NETSURF_QUELLEN").unwrap_or_else(|_| r"C:\ns\3.11".into()));
    assert!(p.join(".fertig").exists(), "NetSurf-Quellen fehlen: python esp32/netsurf/werkzeuge/quellen_holen.py");
    p
}

/// `DIR_SOURCES` aller Makefiles unter `bib/src` (wie `Makefile.subdir`).
fn dir_sources(bib: &Path) -> Vec<PathBuf> {
    let mut v = Vec::new();
    let mut stapel = vec![bib.join("src")];
    while let Some(d) = stapel.pop() {
        let mut eintraege: Vec<_> = std::fs::read_dir(&d).expect("Ordner").flatten().map(|e| e.path()).collect();
        eintraege.sort();
        for e in &eintraege {
            if e.is_dir() {
                stapel.push(e.clone());
            }
        }
        let mk = d.join("Makefile");
        if let Ok(t) = std::fs::read_to_string(&mk) {
            let t = t.replace("\\\r\n", " ").replace("\\\n", " ");
            for z in t.lines() {
                let z = z.trim_start();
                if let Some(rest) = z.strip_prefix("DIR_SOURCES") {
                    let rest = rest.trim_start().trim_start_matches(':').trim_start_matches('+').trim_start_matches('=');
                    v.extend(rest.split_whitespace().filter(|w| w.ends_with(".c")).map(|w| d.join(w)));
                }
            }
        }
    }
    v.sort();
    v
}

/// Wie die Quellen einer Gruppe den Programm-Modus bekommen (32 Bit).
#[derive(Clone, Copy, PartialEq)]
enum Modus {
    /// compat/gemeinsam/ns_programm.h: Speicher aus dem Heap des Programms
    Voll,
    /// nur compat/gemeinsam/ns_abschnitt.h (QuickJS: Speicher über
    /// JS_NewRuntime2, die Makros vertrügen sich nicht mit windows.h)
    Abschnitt,
    /// ohne (c/programm_daten.c: liegt ausserhalb des Abschnitts)
    Ohne,
}

/// Eine Bibliothek: Dateien, Pfade, Defines.
struct Gruppe {
    name: &'static str,
    dateien: Vec<PathBuf>,
    /// nur in den PC-Tests (Proben)
    proben: Vec<PathBuf>,
    pfade: Vec<PathBuf>,
    defines: Vec<(String, Option<String>)>,
    /// compat/esp32os_compat.h voranstellen (nur MSVC)
    msvc_compat: bool,
    modus: Modus,
}

impl Gruppe {
    fn neu(name: &'static str, modus: Modus) -> Self {
        Gruppe { name, dateien: Vec::new(), proben: Vec::new(), pfade: Vec::new(), defines: Vec::new(), msvc_compat: true, modus }
    }
    fn d(&mut self, n: &str, w: Option<&str>) -> &mut Self {
        self.defines.push((n.into(), w.map(Into::into)));
        self
    }
    fn p(&mut self, p: impl AsRef<Path>) -> &mut Self {
        self.pfade.push(p.as_ref().to_path_buf());
        self
    }
    fn f(&mut self, f: impl AsRef<Path>) -> &mut Self {
        self.dateien.push(f.as_ref().to_path_buf());
        self
    }
    /// Defines wie die Upstream-Makefiles (Release, clang/gcc)
    fn standard(&mut self) -> &mut Self {
        self.d("_DEFAULT_SOURCE", None).d("NDEBUG", None);
        self.d("_ALIGNED", Some("__attribute__((aligned))")).d("STMTEXPR", Some("1"));
        self.d("_USE_MATH_DEFINES", None)
    }
}

/// Alle Gruppen. `board`: Einstellungen fürs Board (Ressourcenpfad, keine
/// Proben), sonst für die PC-Tests. `aus`: Ordner für erzeugte Köpfe
/// (dom/bindings/hubbub, vorspann.h).
fn gruppen(q: &Path, aus: &Path, board: bool) -> Vec<Gruppe> {
    let bib = |n: &str| q.join(n);
    let hier = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR"));
    let c = |f: &str| hier.join("c").join(f);
    let inc = aus.join("include");
    let qjs = q.parent().expect("NetSurf-Quellen in einem Ordner").join("quickjs-0.18.0");
    let zlib = q.parent().expect("NetSurf-Quellen in einem Ordner").join("zlib-1.4");
    let mut v = Vec::new();

    // libwapcaplet, libparserutils, libhubbub, libdom (+ Hubbub-Anbindung)
    let mut g = Gruppe::neu("netsurf_dom", Modus::Voll);
    g.p(&inc);
    for n in ["libwapcaplet", "libparserutils", "libhubbub", "libcss", "libdom"] {
        g.p(bib(n).join("include")).p(bib(n).join("src"));
    }
    g.p(bib("libdom")).standard().d("WITHOUT_ICONV_FILTER", None).d("WITH_HUBBUB_BINDING", None);
    for n in ["libwapcaplet", "libparserutils", "libhubbub", "libdom"] {
        for f in dir_sources(&bib(n)) {
            g.f(f);
        }
    }
    g.f(bib("libdom").join("bindings/hubbub/parser.c"));
    g.proben.push(c("probe.c"));
    v.push(g);

    // libcss eigens: gleichnamige Dateien wie libhubbub (charset/detect.c …)
    let mut g = Gruppe::neu("netsurf_css", Modus::Voll);
    for n in ["libwapcaplet", "libparserutils", "libcss"] {
        g.p(bib(n).join("include"));
    }
    g.p(bib("libcss").join("src")).standard();
    let mut autogen: Vec<PathBuf> = std::fs::read_dir(bib("libcss").join("src/parse/properties"))
        .expect("libcss")
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.file_name().and_then(|n| n.to_str()).is_some_and(|n| n.starts_with("autogenerated_") && n.ends_with(".c")))
        .collect();
    autogen.sort();
    for f in dir_sources(&bib("libcss")).into_iter().chain(autogen) {
        g.f(f);
    }
    v.push(g);

    // libnsutils, libnsbmp, libnsgif (je eigene Bibliothek)
    for (n, name) in [("libnsutils", "netsurf_nsutils"), ("libnsbmp", "netsurf_nsbmp"), ("libnsgif", "netsurf_nsgif")] {
        let mut g = Gruppe::neu(name, Modus::Voll);
        g.p(bib(n).join("include")).p(bib(n).join("src")).standard();
        for f in dir_sources(&bib(n)) {
            g.f(f);
        }
        v.push(g);
    }

    // libnsfb: Kern und Plotter; Flächen surface.c + ram.c (die übrigen
    // brauchen SDL, X, VNC, Wayland); „esp32os“ kommt mit dem Frontend
    let mut g = Gruppe::neu("netsurf_nsfb", Modus::Voll);
    let nsfb = bib("libnsfb");
    g.p(nsfb.join("include")).p(nsfb.join("src")).standard();
    for f in dir_sources(&nsfb) {
        g.f(f);
    }
    g.f(nsfb.join("src/surface/surface.c")).f(nsfb.join("src/surface/ram.c"));
    g.proben.push(c("probe_fb.c"));
    v.push(g);

    // QuickJS-ng 0.18.0 als Amalgam
    let mut g = Gruppe::neu("netsurf_quickjs", Modus::Abschnitt);
    g.msvc_compat = false;
    g.p(&qjs).f(qjs.join("quickjs-amalgam.c"));
    g.proben.push(c("probe_js.c"));
    v.push(g);

    // zlib 1.4 (gz*: utils/hashtable.c liest die Messages über gzopen)
    let mut g = Gruppe::neu("netsurf_zlib", Modus::Voll);
    g.msvc_compat = false;
    g.p(&zlib);
    for f in "adler32 crc32 deflate infback inffast inflate inftrees trees zutil compress uncompr gzclose gzlib gzread gzwrite".split_whitespace() {
        g.f(zlib.join(format!("{f}.c")));
    }
    v.push(g);

    let mut g = Gruppe::neu("netsurf_utf8proc", Modus::Voll);
    g.p(bib("libutf8proc").join("include/libutf8proc")).standard().d("UTF8PROC_STATIC", None);
    g.f(bib("libutf8proc").join("src/utf8proc.c"));
    v.push(g);

    // NetSurf-Kern: S_COMMON + S_IMAGE + S_BROWSER der Upstream-Makefiles,
    // kein curl, JavaScript über QuickJS (c/js_quickjs.c), Bilder BMP/ICO/GIF
    let ns = bib("netsurf");
    let mut g = Gruppe::neu("netsurf_kern", Modus::Voll);
    g.p(&inc).p(&ns).p(ns.join("include")).p(ns.join("content/handlers"));
    for n in ["libwapcaplet", "libparserutils", "libhubbub", "libcss", "libdom", "libnsutils", "libnsbmp", "libnsgif"] {
        g.p(bib(n).join("include"));
    }
    g.p(bib("libutf8proc").join("include")).p(&zlib).p(ns.join("frontends")).p(&qjs).p(aus);
    g.standard().d("UTF8PROC_STATIC", None);
    for d in ["WITH_BMP", "WITH_GIF", "WITH_UTF8PROC", "nsframebuffer"] {
        g.d(d, None);
    }
    g.d("NETSURF_HOMEPAGE", Some("\"about:welcome\"")).d("NETSURF_LOG_LEVEL", Some("WARNING"));
    g.d("NETSURF_BUILTIN_LOG_FILTER", Some("\"(level:WARNING || cat:jserrors)\""));
    g.d("NETSURF_BUILTIN_VERBOSE_FILTER", Some("\"(level:VERBOSE || cat:jserrors)\""));
    for (dir, dateien) in KERN {
        for f in dateien.split_whitespace() {
            g.f(ns.join(dir).join(f));
        }
    }
    if board {
        // Board: JS-Laufzeit im 10-MB-Heap des Programms, Stapel 384 KB
        g.d("NS_JS_SPEICHER", Some("(4u*1024u*1024u)")).d("NS_JS_STAPEL", Some("(192u*1024u)"));
    }
    g.f(c("iconv.c")).f(c("js_quickjs.c")).f(c("neu_layout.c"));
    g.proben.push(c("probe_kern.c"));
    v.push(g);

    // Framebuffer-Frontend (S_FRONTEND, fbtk, interne Schrift; Bilder und
    // Schrift erzeugt das Holskript). `main` heisst netsurf_fb_main.
    let fe = ns.join("frontends/framebuffer");
    let erzeugt = q.join("erzeugt");
    let mut g = Gruppe::neu("netsurf_fb", Modus::Voll);
    g.p(&inc).p(&ns).p(ns.join("include")).p(ns.join("content/handlers")).p(ns.join("frontends"));
    for n in ["libwapcaplet", "libparserutils", "libhubbub", "libcss", "libdom", "libnsutils", "libnsfb"] {
        g.p(bib(n).join("include"));
    }
    g.p(&erzeugt).p(bib("libnsfb").join("src")).standard();
    g.d("nsframebuffer", None).d("small", None).d("main", Some("netsurf_fb_main"));
    g.d("NETSURF_LOG_LEVEL", Some("WARNING")).d("NETSURF_HOMEPAGE", Some("\"about:welcome\""));
    // Ressourcen: am Board aus dem Firmware-Abbild über das schreibgeschützte
    // Dateisystem /nsres (firmware/komponenten/esp32os/eingebettet.c), in
    // den PC-Tests über Umgebungsvariablen (je Eintrag expandiert, ':' trennt)
    if board {
        g.d("NETSURF_FB_RESPATH", Some("\"/nsres/\""));
    } else {
        g.d("NETSURF_FB_RESPATH", Some("\"${NETSURF_RES1}:${NETSURF_RES2}\""));
    }
    g.d("NETSURF_FB_FONTPATH", Some("\"/nsres/\""));
    for (n, w) in [
        ("SANS_SERIF", "DejaVuSans.ttf"),
        ("SANS_SERIF_BOLD", "DejaVuSans-Bold.ttf"),
        ("SANS_SERIF_ITALIC", "DejaVuSans-Oblique.ttf"),
        ("SANS_SERIF_ITALIC_BOLD", "DejaVuSans-BoldOblique.ttf"),
        ("SERIF", "DejaVuSerif.ttf"),
        ("SERIF_BOLD", "DejaVuSerif-Bold.ttf"),
        ("MONOSPACE", "DejaVuSansMono.ttf"),
        ("MONOSPACE_BOLD", "DejaVuSansMono-Bold.ttf"),
        ("CURSIVE", "Comic_Sans_MS.ttf"),
        ("FANTASY", "Impact.ttf"),
    ] {
        let w = format!("\"{w}\"");
        g.d(&format!("NETSURF_FB_FONT_{n}"), Some(&w));
    }
    for f in "gui.c framebuffer.c schedule.c bitmap.c fetch.c findfile.c corewindow.c local_history.c clipboard.c font_internal.c".split_whitespace() {
        g.f(fe.join(f));
    }
    for f in "fbtk.c event.c fill.c bitmap.c user.c window.c text.c scroll.c osk.c".split_whitespace() {
        g.f(fe.join("fbtk").join(f));
    }
    g.f(c("getopt.c"));
    let mut erz: Vec<PathBuf> =
        std::fs::read_dir(&erzeugt).expect("erzeugt").flatten().map(|e| e.path()).filter(|p| p.extension().is_some_and(|x| x == "c")).collect();
    erz.sort();
    for f in erz {
        g.f(f);
    }
    // Fläche „esp32os“ (Kern-Fenster) und Einstieg des Programms: nur 32 Bit
    if board || ziel_32() {
        g.f(c("flaeche_esp32os.c"));
    }
    g.proben.push(c("probe_lauf.c"));
    v.push(g);
    v
}

fn ziel_32() -> bool {
    std::env::var("CARGO_CFG_TARGET_POINTER_WIDTH").as_deref() == Ok("32")
}

fn main() {
    println!("cargo:rerun-if-changed=c");
    println!("cargo:rerun-if-changed=compat");
    println!("cargo:rerun-if-changed=js");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("espidf") {
        // Am Board baut ESP-IDF (firmware/komponenten/netsurf) nach der
        // CMake-Liste, die der PC-Bau schreibt; hier nur die Ressourcen
        ressourcen_liste(&quellen());
        return;
    }
    let q = quellen();
    let aus = PathBuf::from(std::env::var("OUT_DIR").expect("OUT_DIR"));
    // Erzeugte Köpfe: Hubbub-Anbindung von libdom (upstream nach dem
    // Installieren unter <dom/bindings/hubbub/…>) und der JS-Vorspann
    let hb = aus.join("include/dom/bindings/hubbub");
    std::fs::create_dir_all(&hb).expect("Ordner");
    for h in ["parser.h", "errors.h"] {
        std::fs::copy(q.join("libdom/bindings/hubbub").join(h), hb.join(h)).expect("Kopie");
    }
    vorspann_erzeugen(&aus.join("vorspann.h"));

    // Für das Board: dieselben Gruppen als CMake-Liste (mit eigenen
    // erzeugten Köpfen neben der Liste)
    board_liste(&q);

    // 32 Bit (i686-PC-Tests): NetSurf als ESP32-OS-Programm
    let programm = ziel_32();
    for g in gruppen(&q, &aus, false) {
        let mut b = cc::Build::new();
        b.compiler("C:/Program Files/LLVM/bin/clang-cl.exe");
        b.flag("-Wno-everything").flag("/O2");
        // POSIX-Kleinigkeiten für die MSVC-Laufzeit (nur NetSurf-Quellen),
        // dann gemeinsame Köpfe (auch am Board)
        if g.msvc_compat {
            b.include("compat");
        }
        b.include("compat/gemeinsam");
        for p in &g.pfade {
            b.include(p);
        }
        for (n, w) in &g.defines {
            b.define(n, w.as_deref());
        }
        if g.msvc_compat {
            b.flag("/FIesp32os_compat.h");
        }
        if programm {
            match g.modus {
                Modus::Voll => {
                    b.flag("/FIns_programm.h");
                }
                Modus::Abschnitt => {
                    b.flag("/FIns_abschnitt.h");
                }
                Modus::Ohne => {}
            }
        }
        for f in g.dateien.iter().chain(&g.proben) {
            b.file(f);
        }
        b.compile(g.name);
    }
    if programm {
        // ohne ns_programm.h: die Sicherung liegt ausserhalb des Abschnitts
        let mut d = cc::Build::new();
        d.compiler("C:/Program Files/LLVM/bin/clang-cl.exe");
        d.flag("-Wno-everything").flag("/O2").file("c/programm_daten.c");
        d.compile("netsurf_programm");
    }
}

/// Ressourcen fürs Board (CSS, Messages de/en, Willkommensseiten, Symbole)
/// als `OUT_DIR/ressourcen.rs`: `DATEIEN`, eine Tabelle im Format von
/// firmware/komponenten/esp32os/eingebettet.c (Name, Zeiger, Länge) mit den
/// eingebetteten Inhalten; das Programm meldet sie als /nsres an.
fn ressourcen_liste(q: &Path) {
    let res = q.join("netsurf/frontends/framebuffer/res");
    let mut liste: Vec<(String, PathBuf)> = Vec::new();
    for f in ["adblock.css", "credits.html", "default.css", "favicon.png", "internal.css", "licence.html", "netsurf.png", "quirks.css", "welcome.html"] {
        liste.push((f.into(), res.join(f)));
    }
    for s in ["de", "en"] {
        liste.push((format!("{s}/welcome.html"), res.join(s).join("welcome.html")));
        liste.push((format!("{s}/Messages"), q.join("erzeugt/res").join(s).join("Messages")));
    }
    let icons = q.join("netsurf/resources/icons");
    let mut png: Vec<PathBuf> =
        std::fs::read_dir(&icons).expect("resources/icons").flatten().map(|e| e.path()).filter(|p| p.extension().is_some_and(|x| x == "png")).collect();
    png.sort();
    for p in png {
        let n = p.file_name().expect("Name").to_string_lossy().into_owned();
        liste.push((format!("icons/{n}"), p));
    }
    let mut t = String::from(
        "// Erzeugt von build.rs (ressourcen_liste).\n\
         /// Eintrag wie `struct eingebettet` in eingebettet.c.\n\
         #[repr(C)]\n\
         pub struct Datei {\n    pub name: *const core::ffi::c_char,\n    pub daten: *const u8,\n    pub laenge: usize,\n}\n\
         // SAFETY: zeigt nur auf unveränderliche Daten des Abbilds.\n\
         unsafe impl Sync for Datei {}\n",
    );
    // je Datei eine Konstante (einmal eingebettet), dann die Tabelle
    for (i, (_, quelle)) in liste.iter().enumerate() {
        assert!(quelle.exists(), "Ressource fehlt: {}", quelle.display());
        println!("cargo:rerun-if-changed={}", quelle.display());
        t.push_str(&format!("const D{i}: &[u8] = include_bytes!({:?});\n", quelle.to_string_lossy().replace('\\', "/")));
    }
    t.push_str(&format!("pub static DATEIEN: [Datei; {}] = [\n", liste.len()));
    for (i, (ziel, _)) in liste.iter().enumerate() {
        t.push_str(&format!("    Datei {{ name: c{ziel:?}.as_ptr(), daten: D{i}.as_ptr(), laenge: D{i}.len() }},\n"));
    }
    t.push_str("];\n");
    let aus = PathBuf::from(std::env::var("OUT_DIR").expect("OUT_DIR"));
    std::fs::write(aus.join("ressourcen.rs"), t).expect("ressourcen.rs");
}

/// `utils/config.h` fürs Board: NetSurf wählt Funktionen nach der
/// Plattform, ESP-IDF kennt es nicht. Die Kopie (vor dem Upstream-Kopf im
/// Suchpfad) nimmt `ESP_PLATFORM` dort auf, wo NetSurf für Plattformen ohne
/// mmap bzw. ohne dirfd/fstatat/unlinkat schon eigene Wege hat (Datei
/// lesen statt abbilden, Pfade statt Verzeichnis-Deskriptoren). Jede
/// Ersetzung prüft den Originaltext; die Upstream-Quellen bleiben
/// unverändert.
fn board_config(q: &Path, ziel: &Path) {
    let quelle = q.join("netsurf/utils/config.h");
    println!("cargo:rerun-if-changed={}", quelle.display());
    let mut t = std::fs::read_to_string(&quelle).expect("utils/config.h").replace("\r\n", "\n");
    for (alt, neu) in [
        ("#define HAVE_MMAP\n#if (defined(_WIN32) ||", "#define HAVE_MMAP\n#if (defined(ESP_PLATFORM) || defined(_WIN32) ||"),
        (
            "#define HAVE_FSTATAT\n#if (defined(_WIN32) ||",
            "#define HAVE_FSTATAT\n#if (defined(ESP_PLATFORM) || defined(_WIN32) ||",
        ),
        // keine Signale unter ESP-IDF (lwIP meldet geschlossene Sockets als
        // Fehler, kein SIGPIPE)
        ("#define HAVE_SIGPIPE\n#if (defined(_WIN32))", "#define HAVE_SIGPIPE\n#if (defined(ESP_PLATFORM) || defined(_WIN32))"),
    ] {
        assert_eq!(t.matches(alt).count(), 1, "utils/config.h: Originaltext nicht (genau einmal) gefunden: {alt:?}");
        t = t.replacen(alt, neu, 1);
    }
    let t = format!("/* Erzeugt von esp32/netsurf/build.rs (board_config) aus utils/config.h. */\n{t}");
    std::fs::create_dir_all(ziel.parent().expect("Ordner")).expect("Ordner");
    if std::fs::read_to_string(ziel).ok().as_deref() != Some(t.as_str()) {
        std::fs::write(ziel, t).expect("config.h");
    }
}

/// CMake-Liste der Gruppen fürs Board: `<Quellen>/erzeugt/netsurf_quellen.cmake`
/// (Variablen NS_QUELLEN, NS_GRUPPE_<name>; Pfade und Defines je Datei).
fn board_liste(q: &Path) {
    let ziel = q.join("erzeugt/board");
    let hb = ziel.join("include/dom/bindings/hubbub");
    std::fs::create_dir_all(&hb).expect("Ordner");
    for h in ["parser.h", "errors.h"] {
        std::fs::copy(q.join("libdom/bindings/hubbub").join(h), hb.join(h)).expect("Kopie");
    }
    vorspann_erzeugen(&ziel.join("vorspann.h"));
    board_config(q, &ziel.join("include/utils/config.h"));
    let hier = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR"));
    let pfad = |p: &Path| p.to_string_lossy().replace('\\', "/");
    let mut t = String::from(
        "# Erzeugt von esp32/netsurf/build.rs – nicht von Hand ändern.\n\
         # Gleiche Quellgruppen wie die PC-Tests; Board: compat/gemeinsam,\n\
         # ns_programm.h vorangestellt (Heap des Programms).\n",
    );
    t.push_str(&format!("set(NS_COMPAT \"{}\")\nset(NS_QUELLEN \"\")\n", pfad(&hier.join("compat/gemeinsam"))));
    for g in gruppen(q, &ziel, true) {
        let n = g.name.to_uppercase();
        t.push_str(&format!("set(NS_GRUPPE_{n}\n"));
        for f in &g.dateien {
            t.push_str(&format!("    \"{}\"\n", pfad(f)));
        }
        t.push_str(")\n");
        // compat/board: was newlib fehlt (sys/utsname.h); dann gemeinsame
        let pfade: Vec<String> = [pfad(&hier.join("compat/board")), pfad(&hier.join("compat/gemeinsam"))]
            .into_iter()
            .chain(g.pfade.iter().map(|p| pfad(p)))
            .collect();
        let defines: Vec<String> = g
            .defines
            .iter()
            .map(|(n, w)| match w {
                Some(w) => format!("{n}={}", w.replace('\\', "\\\\").replace('"', "\\\"")),
                None => n.clone(),
            })
            .collect();
        let optionen = match g.modus {
            Modus::Voll => "-include;ns_programm.h",
            Modus::Abschnitt | Modus::Ohne => "",
        };
        t.push_str(&format!(
            "set_source_files_properties(${{NS_GRUPPE_{n}}} PROPERTIES\n    INCLUDE_DIRECTORIES \"{}\"\n    COMPILE_DEFINITIONS \"{}\"\n    COMPILE_OPTIONS \"{}\")\nlist(APPEND NS_QUELLEN ${{NS_GRUPPE_{n}}})\n",
            pfade.join(";"),
            defines.join(";"),
            optionen
        ));
    }
    let datei = q.join("erzeugt/netsurf_quellen.cmake");
    // nur schreiben, wenn sich etwas ändert (sonst baute ESP-IDF jedes Mal neu)
    if std::fs::read_to_string(&datei).ok().as_deref() != Some(t.as_str()) {
        std::fs::write(&datei, &t).expect("netsurf_quellen.cmake");
    }
    // Stand der erzeugten Dateien in den Komponentenordner: cargo beobachtet
    // für den Board-Bau nur die Komponentenordner, nicht C:\ns – ohne diese
    // Datei bliebe eine geänderte Liste oder ein geänderter Kopf unbemerkt
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    let mut mischen = |b: &[u8]| {
        for &x in b {
            h = (h ^ u64::from(x)).wrapping_mul(0x100_0000_01b3);
        }
    };
    mischen(t.as_bytes());
    for f in ["vorspann.h", "include/utils/config.h", "include/dom/bindings/hubbub/parser.h", "include/dom/bindings/hubbub/errors.h"] {
        mischen(&std::fs::read(ziel.join(f)).expect("erzeugte Datei"));
    }
    // auch der Inhalt aller Quellen (eigene in netsurf/c, erzeugte Bilder …)
    // und der eigenen Köpfe
    for g in gruppen(q, &ziel, true) {
        for f in &g.dateien {
            mischen(&std::fs::read(f).expect("Quelle"));
        }
    }
    // c/programm_daten.c baut die Komponente direkt (ausserhalb der Gruppen)
    mischen(&std::fs::read(hier.join("c/programm_daten.c")).expect("programm_daten.c"));
    for d in ["compat/gemeinsam", "compat/board", "compat/board/sys", "compat/gemeinsam/curl"] {
        let mut k: Vec<PathBuf> = std::fs::read_dir(hier.join(d)).expect("compat").flatten().map(|e| e.path()).filter(|p| p.is_file()).collect();
        k.sort();
        for f in k {
            mischen(&std::fs::read(f).expect("Kopf"));
        }
    }
    let stand = hier.join("../firmware/komponenten/netsurf/quellen_stand.txt");
    let neu = format!("{h:016x}\n");
    if std::fs::read_to_string(&stand).ok().as_deref() != Some(neu.as_str()) {
        std::fs::write(&stand, neu).expect("quellen_stand.txt");
    }
}

/// Quellen des NetSurf-Kerns (Ordner, Dateien) – aus content/Makefile,
/// content/fetchers/…, content/handlers/…, utils/…, desktop/Makefile.
const KERN: &[(&str, &str)] = &[
    ("content", "content.c content_factory.c fetch.c hlcache.c llcache.c mimesniff.c textsearch.c urldb.c no_backing_store.c"),
    ("content/fetchers", "data.c resource.c"),
    (
        "content/fetchers/about",
        "about.c blank.c certificate.c chart.c choices.c config.c imagecache.c nscolours.c query.c query_auth.c query_fetcherror.c \
         query_privacy.c query_timeout.c testament.c",
    ),
    ("content/fetchers/file", "dirlist.c file.c"),
    ("content/handlers/css", "css.c dump.c internal.c hints.c select.c"),
    // JavaScript: QuickJS-ng statt Duktape (c/js_quickjs.c, STATUS
    // „Browser-Architektur“); content.c = Inhaltstyp für externe Skripte
    ("content/handlers/javascript", "fetcher.c content.c"),
    (
        "content/handlers/html",
        "box_construct.c box_inspect.c box_manipulate.c box_normalise.c box_special.c box_textarea.c css.c css_fetcher.c \
         dom_event.c font.c form.c forms.c html.c imagemap.c interaction.c layout.c layout_flex.c object.c redraw.c \
         redraw_border.c script.c table.c textselection.c",
    ),
    ("content/handlers/text", "textplain.c"),
    ("content/handlers/image", "image.c image_cache.c bmp.c gif.c ico.c"),
    (
        "utils",
        "bloom.c corestrings.c file.c filename.c filepath.c hashmap.c hashtable.c idna.c libdom.c log.c messages.c nscolour.c \
         nsoption.c punycode.c ssl_certs.c talloc.c time.c url.c useragent.c utf8.c utils.c",
    ),
    (
        "utils/http",
        "challenge.c generics.c primitives.c parameter.c cache-control.c content-disposition.c content-type.c \
         strict-transport-security.c www-authenticate.c",
    ),
    ("utils/nsurl", "nsurl.c parse.c"),
    (
        "desktop",
        "cookie_manager.c knockout.c hotlist.c mouse.c plot_style.c print.c search.c searchweb.c scrollbar.c textarea.c \
         version.c system_colour.c local_history.c global_history.c treeview.c page-info.c \
         bitmap.c browser.c browser_window.c browser_history.c download.c frames.c netsurf.c cw_helper.c \
         save_text.c selection.c textinput.c gui_factory.c save_pdf.c font_haru.c",
        // save_complete.c fehlt absichtlich: braucht POSIX-Regex (weder MSVC
        // noch newlib), und das Framebuffer-Frontend bietet „komplett
        // speichern“ nicht an (PLAN, Abweichung „B1 komplett speichern“)
    ),
];

/// `js/vorspann.js` als nullterminiertes C-Feld (QuickJS verlangt das
/// Nullbyte; `sizeof - 1` ist die Länge).
fn vorspann_erzeugen(ziel: &Path) {
    let js = std::fs::read("js/vorspann.js").expect("js/vorspann.js");
    let mut t = String::from("/* erzeugt von build.rs aus js/vorspann.js */\nstatic const char ns_vorspann[] = {\n");
    for z in js.chunks(24) {
        for b in z {
            t.push_str(&format!("{b},"));
        }
        t.push('\n');
    }
    t.push_str("0};\n");
    if std::fs::read_to_string(ziel).ok().as_deref() != Some(t.as_str()) {
        std::fs::write(ziel, t).expect("vorspann.h");
    }
}
