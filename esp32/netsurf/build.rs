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
#[derive(Clone)]
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
    g.d("NETSURF_HOMEPAGE", Some("\"resource:welcome.html\"")).d("NETSURF_LOG_LEVEL", Some("WARNING"));
    g.d("NETSURF_BUILTIN_LOG_FILTER", Some("\"(level:WARNING || cat:jserrors)\""));
    g.d("NETSURF_BUILTIN_VERBOSE_FILTER", Some("\"(level:VERBOSE || cat:jserrors)\""));
    for (dir, dateien) in KERN {
        for f in dateien.split_whitespace() {
            g.f(ns.join(dir).join(f));
        }
    }
    if board {
        // Board: JS-Laufzeit im 10-MB-Heap des Programms, Stapel 384 KB.
        // 2 MB: ein JS-lastiger erster Versuch (Wikipedia/MediaWiki) darf den
        // Heap nicht so füllen, dass die Seite danach ohne JS nicht mehr
        // passt (gemessen 22.11.: 4 MB → Heap voll beim Neuladen)
        g.d("NS_JS_SPEICHER", Some("(2u*1024u*1024u)")).d("NS_JS_STAPEL", Some("(192u*1024u)"));
    }
    g.f(c("iconv.c")).f(c("js_quickjs.c")).f(c("neu_layout.c")).f(c("eingabe.c")).f(c("ohne_js.c")).f(c("lesezeichen.c")).f(c("touch.c"));
    // html.c als geprüfte Kopie: Seiten auf der Liste „ohne JavaScript“
    // (c/ohne_js.c) laden ohne Skripte
    g.f(html_kopie(q, aus));
    // llcache.c als geprüfte Kopie: Grösse des Zwischenspeichers nach jedem
    // Aufräumen messbar (B3)
    g.f(llcache_kopie(q, aus));
    g.proben.push(c("probe_kern.c"));
    // B2: content/fetch.c mit WITH_CURL (ruft fetch_curl_register, das
    // c/abruf.c bereitstellt) – nur diese Datei, utils/time.c bliebe sonst
    // bei curl_getdate; gleiche Einstellungen wie der Kern
    let mut a = Gruppe::neu("netsurf_abruf", Modus::Voll);
    a.pfade = g.pfade.clone();
    a.defines = g.defines.clone();
    a.d("WITH_CURL", None);
    a.f(ns.join("content/fetch.c")).f(c("abruf.c"));
    v.push(g);
    v.push(a);
    // Transport: am Board esp_http_client (ausserhalb des Neustart-Abschnitts,
    // netsurf.lf), am PC http über Winsock zum Prüfserver (ohne
    // ns_programm.h: eigener Speicher der C-Laufzeit)
    if board {
        let mut t = Gruppe::neu("netsurf_transport", Modus::Voll);
        t.msvc_compat = false;
        t.f(c("transport_esp.c"));
        v.push(t);
    } else {
        let mut t = Gruppe::neu("netsurf_transport", Modus::Ohne);
        t.msvc_compat = false;
        t.f(c("transport_pc.c"));
        v.push(t);
    }

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
    g.d("NETSURF_LOG_LEVEL", Some("WARNING")).d("NETSURF_HOMEPAGE", Some("\"resource:welcome.html\""));
    // Ressourcen: am Board aus dem Firmware-Abbild über das schreibgeschützte
    // Dateisystem /nsres (firmware/komponenten/esp32os/eingebettet.c), in
    // den PC-Tests über Umgebungsvariablen (je Eintrag expandiert, ':' trennt)
    if board {
        g.d("NETSURF_FB_RESPATH", Some("\"/nsres/\""));
        // B3: Zwischenspeicher (llcache) 2 MB statt 12 MB – NetSurfs
        // Untergrenze; davon 25 % für Bilder. Der Browser hat 10 MB Heap.
        g.d("NS_SPEICHER_CACHE", Some("(2*1024*1024)"));
    } else {
        g.d("NETSURF_FB_RESPATH", Some("\"${NETSURF_RES3}:${NETSURF_RES1}:${NETSURF_RES2}\""));
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
    // gui.c als geprüfte Kopie (Suche aus der Adressleiste, Suchanbieter)
    g.f(gui_kopie(q, aus));
    for f in "framebuffer.c schedule.c bitmap.c fetch.c findfile.c corewindow.c local_history.c clipboard.c font_internal.c".split_whitespace() {
        g.f(fe.join(f));
    }
    // fbtk.c als geprüfte Kopie (B6: Schreibmarke → Bildschirmtastatur);
    // sie bindet widget.h aus dem fbtk-Ordner ein
    g.p(fe.join("fbtk"));
    g.f(fbtk_kopie(q, aus));
    for f in "event.c fill.c bitmap.c user.c window.c text.c scroll.c osk.c".split_whitespace() {
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
    // Transport der PC-Tests (c/transport_pc.c)
    println!("cargo:rustc-link-lib=ws2_32");
}

/// Ressourcen fürs Board (CSS, Messages de/en, Willkommensseiten, Symbole)
/// als `OUT_DIR/ressourcen.rs`: `DATEIEN`, eine Tabelle im Format von
/// firmware/komponenten/esp32os/eingebettet.c (Name, Zeiger, Länge) mit den
/// eingebetteten Inhalten; das Programm meldet sie als /nsres an.
fn ressourcen_liste(q: &Path) {
    let res = q.join("netsurf/frontends/framebuffer/res");
    let mut liste: Vec<(String, PathBuf)> = Vec::new();
    for f in ["adblock.css", "credits.html", "default.css", "favicon.png", "internal.css", "licence.html", "netsurf.png", "quirks.css"] {
        liste.push((f.into(), res.join(f)));
    }
    for s in ["de", "en"] {
        liste.push((format!("{s}/Messages"), q.join("erzeugt/res").join(s).join("Messages")));
    }
    // eigene Ressourcen (netsurf/res): Startseite mit Suchfeld (ersetzt die
    // welcome.html des Frontends – resource: kennt nur feste Namen),
    // Suchanbieter
    let eigen = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR")).join("res");
    println!("cargo:rerun-if-changed={}", eigen.display());
    for f in ["SearchEngines", "welcome.html", "de/welcome.html", "en/welcome.html"] {
        liste.push((f.into(), eigen.join(f)));
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

/// `frontends/framebuffer/gui.c` als geprüfte Kopie in `ziel` (B2):
/// - die Adressleiste deutet Eingaben über `ns_eingabe_zu_url` (c/eingabe.c):
///   Adresse oder – bei Wörtern – Suche beim eingestellten Anbieter;
/// - nach `netsurf_init` die Suchanbieter aus der Ressource `SearchEngines`
///   (eigene Liste in netsurf/res, DuckDuckGo-HTML zuerst).
/// Jede Ersetzung prüft den Originaltext; das Original bleibt unverändert.
fn gui_kopie(q: &Path, ziel: &Path) -> PathBuf {
    let quelle = q.join("netsurf/frontends/framebuffer/gui.c");
    println!("cargo:rerun-if-changed={}", quelle.display());
    let mut t = std::fs::read_to_string(&quelle).expect("gui.c").replace("\r\n", "\n");
    for (alt, neu) in [
        (
            "#include \"framebuffer/local_history.h\"\n",
            "#include \"framebuffer/local_history.h\"\n#include \"desktop/searchweb.h\"\n\
             /* ESP32-OS (B2): Eingabe der Adressleiste → Adresse oder Suche */\n\
             nserror ns_eingabe_zu_url(const char *text, struct nsurl **url);\n\
             /* ESP32-OS (B4): Verlauf und Lesezeichen speichern (gui_quit) */\n\
             void ns_daten_ende(void);\n\
             /* ESP32-OS (B5): Touch-Scrollen und Pinch-Zoom (c/touch.c) */\n\
             int ns_touch_scroll_nehmen(void);\n\
             bool ns_touch_zoom_nehmen(double *faktor);\n",
        ),
        ("\terror = nsurl_create(text, &url);\n", "\terror = ns_eingabe_zu_url(text, &url);\n"),
        // B4: Adressleiste – erstes Antippen leert sie (Touch: kein Markieren;
        // die Bildschirmtastatur hat keine Ende-Taste)
        (
            "/* reload icon click routine */\n",
            "/* ESP32-OS (B4): erstes Antippen der Adressleiste leert sie, damit
             sich gleich eine neue Adresse tippen lässt; ein weiteres Antippen
             setzt die Schreibmarke wie gewohnt */\n\
             static fbtk_callback ns_url_klick_alt;\n\n\
             static int\nns_url_klick(fbtk_widget_t *widget, fbtk_callback_info *cbi)\n{\n\
             \tint x, y, h;\n\
             \tif (!fbtk_get_caret(widget, &x, &y, &h))\n\t\tfbtk_set_text(widget, \"\");\n\
             \treturn ns_url_klick_alt != NULL ? ns_url_klick_alt(widget, cbi) : 0;\n}\n\n\
             /* reload icon click routine */\n",
        ),
        (
            "\t\t\t\tgw->url = widget; /* keep reference */\n",
            "\t\t\t\tgw->url = widget; /* keep reference */\n\
             \t\t\t\tns_url_klick_alt = fbtk_set_handler(widget, FBTK_CBT_CLICK, ns_url_klick, NULL);\n",
        ),
        // B5: Wischen scrollt um den Fingerweg (gesammelte Pixel aus
        // c/touch.c), ein echtes Mausrad weiter um 100
        (
            "\t\tcase NSFB_KEY_MOUSE_4:\n\t\t\t/* scroll up */\n\t\t\tif (browser_window_scroll_at_point(gw->bw,\n\t\t\t\t\t\t\t   x, y,\n\t\t\t\t\t\t\t   0, -100) == false)\n\t\t\t\twidget_scroll_y(gw, -100, false);\n\t\t\tbreak;\n\n\t\tcase NSFB_KEY_MOUSE_5:\n\t\t\t/* scroll down */\n\t\t\tif (browser_window_scroll_at_point(gw->bw,\n\t\t\t\t\t\t\t   x, y,\n\t\t\t\t\t\t\t   0, 100) == false)\n\t\t\t\twidget_scroll_y(gw, 100, false);\n\t\t\tbreak;\n",
            "\t\tcase NSFB_KEY_MOUSE_4:\n\t\tcase NSFB_KEY_MOUSE_5: {\n\
             \t\t\t/* ESP32-OS (B5): Fingerweg aus c/touch.c, sonst Mausrad */\n\
             \t\t\tint d = ns_touch_scroll_nehmen();\n\
             \t\t\tif (d == 0)\n\
             \t\t\t\td = cbi->event->value.keycode == NSFB_KEY_MOUSE_4 ? -100 : 100;\n\
             \t\t\tif (browser_window_scroll_at_point(gw->bw, x, y, 0, d) == false)\n\
             \t\t\t\twidget_scroll_y(gw, d, false);\n\
             \t\t\tbreak;\n\t\t}\n",
        ),
        // B5: Pinch-Zoom nach jedem Ereignis-Durchlauf (0,5- bis 3-fach)
        (
            "\t\tfbtk_redraw(fbtk);\n\t}\n}\n",
            "\t\t/* ESP32-OS (B5): Pinch-Zoom (c/touch.c) */\n\
             \t\t{\n\t\t\tdouble f;\n\
             \t\t\tif (ns_touch_zoom_nehmen(&f) && window_list != NULL) {\n\
             \t\t\t\tfloat s = browser_window_get_scale(window_list->bw) * (float)f;\n\
             \t\t\t\tif (s < 0.5f)\n\t\t\t\t\ts = 0.5f;\n\
             \t\t\t\tif (s > 3.0f)\n\t\t\t\t\ts = 3.0f;\n\
             \t\t\t\tbrowser_window_set_scale(window_list->bw, s, true);\n\
             \t\t\t}\n\t\t}\n\
             \t\tfbtk_redraw(fbtk);\n\t}\n}\n",
        ),
        // B3: Zwischenspeicher am Board begrenzen (vor netsurf_init)
        (
            "\tnsoption_commandline(&argc, argv, nsoptions);\n",
            "\tnsoption_commandline(&argc, argv, nsoptions);\n\
             #ifdef NS_SPEICHER_CACHE\n\
             \t/* ESP32-OS (B3): Zwischenspeicher begrenzt (Heap des Browsers 10 MB) */\n\
             \tnsoption_set_int(memory_cache_size, NS_SPEICHER_CACHE);\n\
             #endif\n\
             \t/* ESP32-OS (B3, PC-Tests): Grenze per Umgebung */\n\
             \t{\n\t\tconst char *grenze = getenv(\"NS_CACHE_GRENZE\");\n\
             \t\tif (grenze != NULL && grenze[0] != '\\0')\n\
             \t\t\tnsoption_set_int(memory_cache_size, atoi(grenze));\n\t}\n\
             \t/* ESP32-OS (B4): Datenordner, Cookie-Dateien */\n\
             \tns_daten_optionen();\n",
        ),
        // Knopf „Ohne JS“/„Mit JS“ (c/ohne_js.c) in der Werkzeugleiste
        ("#define NSFB_TOOLBAR_DEFAULT_LAYOUT \"blfsrutc\"", "#define NSFB_TOOLBAR_DEFAULT_LAYOUT \"blfsrjmvutc\""),
        (
            "\tbrowser_window_reload(bw, true);\n\treturn 1;\n}\n",
            "\tbrowser_window_reload(bw, true);\n\treturn 1;\n}\n\n\
             /* ESP32-OS: Seite ohne bzw. wieder mit JavaScript laden (c/ohne_js.c) */\n\
             void ns_ohne_js_umschalten(struct browser_window *bw);\n\
             const char *ns_ohne_js_knopftext(struct nsurl *url);\n\
             void ns_ohne_js_ladestart(struct browser_window *bw);\n\
             #define NS_JS_KNOPF_BREITE 70\n\
             static fbtk_widget_t *ns_js_knopf;\n\nvoid ns_js_knopf_text(const char *text)\n{\n\tif (ns_js_knopf != NULL)\n\t\tfbtk_set_text(ns_js_knopf, text);\n}\n\n\
             /* ESP32-OS (B4): Lesezeichen und Verlauf (c/lesezeichen.c) */\n\
             void ns_lesezeichen_umschalten(struct browser_window *bw);\n\
             const char *ns_merken_knopftext(struct nsurl *url);\n\
             void ns_liste_oeffnen(struct browser_window *bw);\n\
             void ns_daten_optionen(void);\n\
             void ns_daten_start(void);\n\
             void ns_daten_ende(void);\n\
             #define NS_MERKEN_BREITE 70\n\
             #define NS_LISTE_BREITE 54\n\
             static fbtk_widget_t *ns_merken_knopf;\n\n\
             void ns_merken_knopf_text(const char *text)\n{\n\
             \tif (ns_merken_knopf != NULL)\n\t\tfbtk_set_text(ns_merken_knopf, text);\n}\n\n\
             static int\nns_merken_klick(fbtk_widget_t *widget, fbtk_callback_info *cbi)\n{\n\
             \tif (cbi->event->type != NSFB_EVENT_KEY_UP)\n\t\treturn 0;\n\
             \tns_lesezeichen_umschalten(cbi->context);\n\treturn 1;\n}\n\n\
             static int\nns_liste_klick(fbtk_widget_t *widget, fbtk_callback_info *cbi)\n{\n\
             \tif (cbi->event->type != NSFB_EVENT_KEY_UP)\n\t\treturn 0;\n\
             \tns_liste_oeffnen(cbi->context);\n\treturn 1;\n}\n\n\
             static int\nns_js_knopf_klick(fbtk_widget_t *widget, fbtk_callback_info *cbi)\n{\n\
             \tif (cbi->event->type != NSFB_EVENT_KEY_UP)\n\t\treturn 0;\n\
             \tns_ohne_js_umschalten(cbi->context);\n\treturn 1;\n}\n",
        ),
        (
            "\t\tcase 't': /* throbber/activity indicator */\n\t\t\twidget = fbtk_create_bitmap(",
            "\t\tcase 'j': /* ESP32-OS: ohne/mit JavaScript */\n\
             \t\t\twidget = fbtk_create_text_button(toolbar,\n\
             \t\t\t\t\t\t(xdir == 1) ? xpos : xpos - NS_JS_KNOPF_BREITE,\n\
             \t\t\t\t\t\tpadding, NS_JS_KNOPF_BREITE, -padding,\n\
             \t\t\t\t\t\tframe_col, FB_COLOUR_BLACK,\n\
             \t\t\t\t\t\tns_js_knopf_klick, gw->bw);\n\
             \t\t\tfbtk_set_text(widget, \"Ohne JS\");\n\
             \t\t\tns_js_knopf = widget;\n\t\t\tbreak;\n\n\
             \t\tcase 't': /* throbber/activity indicator */\n\t\t\twidget = fbtk_create_bitmap(",
        ),
        (
            "\t\tcase 't': /* throbber/activity indicator */\n\t\t\twidget = fbtk_create_bitmap(",
            "\t\tcase 'm': /* ESP32-OS (B4): Lesezeichen merken */\n\
             \t\t\twidget = fbtk_create_text_button(toolbar,\n\
             \t\t\t\t\t\t(xdir == 1) ? xpos : xpos - NS_MERKEN_BREITE,\n\
             \t\t\t\t\t\tpadding, NS_MERKEN_BREITE, -padding,\n\
             \t\t\t\t\t\tframe_col, FB_COLOUR_BLACK,\n\
             \t\t\t\t\t\tns_merken_klick, gw->bw);\n\
             \t\t\tfbtk_set_text(widget, ns_merken_knopftext(NULL));\n\
             \t\t\tns_merken_knopf = widget;\n\t\t\tbreak;\n\n\
             \t\tcase 'v': /* ESP32-OS (B4): Lesezeichen und Verlauf */\n\
             \t\t\twidget = fbtk_create_text_button(toolbar,\n\
             \t\t\t\t\t\t(xdir == 1) ? xpos : xpos - NS_LISTE_BREITE,\n\
             \t\t\t\t\t\tpadding, NS_LISTE_BREITE, -padding,\n\
             \t\t\t\t\t\tframe_col, FB_COLOUR_BLACK,\n\
             \t\t\t\t\t\tns_liste_klick, gw->bw);\n\
             \t\t\tfbtk_set_text(widget, getenv(\"LANGUAGE\") != NULL && strncmp(getenv(\"LANGUAGE\"), \"de\", 2) == 0 ? \"Liste\" : \"List\");\n\
             \t\t\tbreak;\n\n\
             \t\tcase 't': /* throbber/activity indicator */\n\t\t\twidget = fbtk_create_bitmap(",
        ),
        (
            "\t\tcase 't': /* throbber/activity indicator */\n\t\t\twidget = gw->throbber;\n",
            "\t\tcase 'j': /* ESP32-OS: ohne/mit JavaScript */\n\
             \t\t\twidget = ns_js_knopf;\n\
             \t\t\tx = (xdir == 1) ? xpos : xpos - NS_JS_KNOPF_BREITE;\n\
             \t\t\ty = padding;\n\t\t\tw = NS_JS_KNOPF_BREITE;\n\t\t\th = -padding;\n\t\t\tbreak;\n\n\
             \t\tcase 't': /* throbber/activity indicator */\n\t\t\twidget = gw->throbber;\n",
        ),
        (
            "\t\tcase 'j': /* ESP32-OS: ohne/mit JavaScript */\n\t\t\twidget = ns_js_knopf;\n",
            "\t\tcase 'm': /* ESP32-OS (B4) */\n\
             \t\t\twidget = ns_merken_knopf;\n\
             \t\t\tx = (xdir == 1) ? xpos : xpos - NS_MERKEN_BREITE;\n\
             \t\t\ty = padding;\n\t\t\tw = NS_MERKEN_BREITE;\n\t\t\th = -padding;\n\t\t\tbreak;\n\n\
             \t\tcase 'v': /* ESP32-OS (B4): Knopf ohne Merker – nicht verschieben */\n\
             \t\t\twidget = NULL;\n\
             \t\t\tw = NS_LISTE_BREITE;\n\t\t\tbreak;\n\n\
             \t\tcase 'j': /* ESP32-OS: ohne/mit JavaScript */\n\t\t\twidget = ns_js_knopf;\n",
        ),
        (
            "\tfbtk_set_text(g->url, nsurl_access(url));\n\treturn NSERROR_OK;\n",
            "\tfbtk_set_text(g->url, nsurl_access(url));\n\
             \tif (ns_js_knopf != NULL)\n\t\tfbtk_set_text(ns_js_knopf, ns_ohne_js_knopftext(url));\n\
             \tif (ns_merken_knopf != NULL)\n\t\tfbtk_set_text(ns_merken_knopf, ns_merken_knopftext(url));\n\
             \treturn NSERROR_OK;\n",
        ),
        (
            "\tg->throbber_index = 0;\n\tframebuffer_schedule(100, throbber_advance, g);\n",
            "\tg->throbber_index = 0;\n\tframebuffer_schedule(100, throbber_advance, g);\n\
             \t/* ESP32-OS: Rückfall ohne JavaScript, wenn die Seite nicht erscheint */\n\
             \tns_ohne_js_ladestart(g->bw);\n",
        ),
        (
            "\tret = netsurf_init(NULL);\n\tif (ret != NSERROR_OK) {\n\t\tdie(\"NetSurf failed to initialise\");\n\t}\n",
            "\tret = netsurf_init(NULL);\n\tif (ret != NSERROR_OK) {\n\t\tdie(\"NetSurf failed to initialise\");\n\t}\n\n\
             \t/* ESP32-OS (B2): Suchanbieter (SearchEngines, DuckDuckGo-HTML zuerst) */\n\
             \t{\n\t\tchar *anbieter = filepath_find(respaths, \"SearchEngines\");\n\
             \t\tsearch_web_init(anbieter);\n\t\tfree(anbieter);\n\
             \t\tsearch_web_select_provider(nsoption_int(search_provider));\n\t}\n\
             \t/* ESP32-OS (B4): Adressen (Verlauf) und Lesezeichen laden */\n\
             \tns_daten_start();\n",
        ),
        (
            "\turldb_save_cookies(nsoption_charp(cookie_jar));\n\n\tframebuffer_finalise();\n",
            "\t/* ESP32-OS (B4): Verlauf und Lesezeichen speichern */\n\
             \tns_daten_ende();\n\
             \turldb_save_cookies(nsoption_charp(cookie_jar));\n\n\tframebuffer_finalise();\n",
        ),
        // B4: nach jeder neuen Adresse gibt die Adressleiste die Schreibmarke
        // ab – das nächste Antippen leert sie wieder (sonst landete neuer Text
        // mitten in der alten Adresse)
        (
            "\tif (ns_merken_knopf != NULL)\n\t\tfbtk_set_text(ns_merken_knopf, ns_merken_knopftext(url));\n",
            "\tif (ns_merken_knopf != NULL)\n\t\tfbtk_set_text(ns_merken_knopf, ns_merken_knopftext(url));\n\
             \t{\n\t\tint cx, cy, ch;\n\
             \t\tif (fbtk_get_caret(g->url, &cx, &cy, &ch))\n\
             \t\t\tfbtk_set_caret(g->url, false, 0, 0, 0, NULL);\n\t}\n",
        ),
    ] {
        assert_eq!(t.matches(alt).count(), 1, "gui.c: Originaltext nicht (genau einmal) gefunden: {alt:?}");
        t = t.replacen(alt, neu, 1);
    }
    let t = format!("/* Erzeugt von esp32/netsurf/build.rs (gui_kopie) aus frontends/framebuffer/gui.c. */\n{t}");
    let datei = ziel.join("gui.c");
    std::fs::create_dir_all(ziel).expect("Ordner");
    if std::fs::read_to_string(&datei).ok().as_deref() != Some(t.as_str()) {
        std::fs::write(&datei, t).expect("gui.c");
    }
    datei
}

/// `content/handlers/html/html.c` als geprüfte Kopie: eine Seite auf der
/// Liste „ohne JavaScript“ (c/ohne_js.c, Rückfallebene und Knopf) wird ohne
/// Skripte angelegt (`enable_scripting`, wirkt auf Parser, Skripte und
/// `<noscript>`).
fn html_kopie(q: &Path, ziel: &Path) -> PathBuf {
    let quelle = q.join("netsurf/content/handlers/html/html.c");
    println!("cargo:rerun-if-changed={}", quelle.display());
    let mut t = std::fs::read_to_string(&quelle).expect("html.c").replace("\r\n", "\n");
    for (alt, neu) in [
        (
            "#include \"html/layout.h\"\n",
            "#include \"html/layout.h\"\n\
             /* ESP32-OS: Seiten ohne JavaScript (c/ohne_js.c) */\n\
             bool ns_ohne_js(struct nsurl *url);\n",
        ),
        (
            "\tc->enable_scripting = nsoption_bool(enable_javascript);\n",
            "\tc->enable_scripting = nsoption_bool(enable_javascript) && !ns_ohne_js(content_get_url(&c->base));\n",
        ),
    ] {
        assert_eq!(t.matches(alt).count(), 1, "html.c: Originaltext nicht (genau einmal) gefunden: {alt:?}");
        t = t.replacen(alt, neu, 1);
    }
    let t = format!("/* Erzeugt von esp32/netsurf/build.rs (html_kopie) aus content/handlers/html/html.c. */\n{t}");
    let datei = ziel.join("html.c");
    std::fs::create_dir_all(ziel).expect("Ordner");
    if std::fs::read_to_string(&datei).ok().as_deref() != Some(t.as_str()) {
        std::fs::write(&datei, t).expect("html.c");
    }
    datei
}

/// `frontends/framebuffer/fbtk/fbtk.c` als geprüfte Kopie (B6): jede
/// Schreibmarke – Formularfeld einer Seite oder Adressleiste – läuft über
/// `fbtk_set_caret`; dort meldet die Kopie dem Kern, ob ein Textfeld den
/// Fokus hat und wo es liegt (Bildschirmtastatur erscheint, verdeckt das
/// Feld nicht, verschwindet wieder).
fn fbtk_kopie(q: &Path, ziel: &Path) -> PathBuf {
    let quelle = q.join("netsurf/frontends/framebuffer/fbtk/fbtk.c");
    println!("cargo:rerun-if-changed={}", quelle.display());
    let mut t = std::fs::read_to_string(&quelle).expect("fbtk.c").replace("\r\n", "\n");
    for (alt, neu) in [
        (
            "#include \"widget.h\"\n",
            "#include \"widget.h\"\n\n\
             /* ESP32-OS (B6): Textfeld-Fokus an den Kern (netsurf/src/lib.rs) */\n\
             void ns_tastatur_fokus(int an, int x, int y, int hoehe);\n",
        ),
        (
            "\t} else {\n\t\troot->u.root.caret.owner = NULL;\n\t\troot->u.root.caret.remove_cb = NULL;\n\t}\n}\n",
            "\t} else {\n\t\troot->u.root.caret.owner = NULL;\n\t\troot->u.root.caret.remove_cb = NULL;\n\t}\n\
             \t/* ESP32-OS (B6): Bildschirmtastatur – Lage im Fenster */\n\
             \tns_tastatur_fokus(set, fbtk_get_absx(widget) + x, fbtk_get_absy(widget) + y, height);\n}\n",
        ),
    ] {
        assert_eq!(t.matches(alt).count(), 1, "fbtk.c: Originaltext nicht (genau einmal) gefunden: {alt:?}");
        t = t.replacen(alt, neu, 1);
    }
    let t = format!("/* Erzeugt von esp32/netsurf/build.rs (fbtk_kopie) aus frontends/framebuffer/fbtk/fbtk.c. */\n{t}");
    let datei = ziel.join("fbtk.c");
    std::fs::create_dir_all(ziel).expect("Ordner");
    if std::fs::read_to_string(&datei).ok().as_deref() != Some(t.as_str()) {
        std::fs::write(&datei, t).expect("fbtk.c");
    }
    datei
}

/// `content/llcache.c` als geprüfte Kopie (B3): am Ende jedes Aufräumens
/// (`llcache_clean`) Gesamtgrösse, Grenze und den freigebbaren Teil
/// (Objekte ohne Nutzer und ohne laufenden Abruf) festhalten – die Grenze
/// gilt für diesen Teil; was eine Seite gerade nutzt, bleibt.
fn llcache_kopie(q: &Path, ziel: &Path) -> PathBuf {
    let quelle = q.join("netsurf/content/llcache.c");
    println!("cargo:rerun-if-changed={}", quelle.display());
    let mut t = std::fs::read_to_string(&quelle).expect("llcache.c").replace("\r\n", "\n");
    for (alt, neu) in [
        (
            "\nvoid llcache_clean(bool purge)\n{\n",
            "\n/* ESP32-OS (B3): Messwerte des letzten Aufräumens (Board-Log\n \
             * „ns_takt“ bzw. Tests lesen sie über ns_llcache_stand) */\n\
             static uint32_t ns_cache_groesse, ns_cache_grenze, ns_cache_frei;\n\
             void ns_llcache_stand(uint32_t *groesse, uint32_t *grenze, uint32_t *frei)\n{\n\
             \t*groesse = ns_cache_groesse;\n\t*grenze = ns_cache_grenze;\n\t*frei = ns_cache_frei;\n}\n\n\
             void llcache_clean(bool purge)\n{\n",
        ),
        (
            "\tNSLOG(llcache, DEBUG, \"Size: %\"PRIu32\" (limit: %\"PRIu32\")\", llcache_size, limit);\n}\n",
            "\tNSLOG(llcache, DEBUG, \"Size: %\"PRIu32\" (limit: %\"PRIu32\")\", llcache_size, limit);\n\
             \t/* ESP32-OS (B3): Messwerte festhalten */\n\
             \t{\n\
             \t\tuint32_t frei = 0;\n\
             \t\tfor (object = llcache->cached_objects; object != NULL; object = object->next)\n\
             \t\t\tif (object->users == NULL && object->candidate_count == 0 && object->fetch.fetch == NULL)\n\
             \t\t\t\tfrei += total_object_size(object);\n\
             \t\tfor (object = llcache->uncached_objects; object != NULL; object = object->next)\n\
             \t\t\tif (object->users == NULL && object->candidate_count == 0 && object->fetch.fetch == NULL)\n\
             \t\t\t\tfrei += total_object_size(object);\n\
             \t\tns_cache_groesse = llcache_size;\n\
             \t\tns_cache_grenze = limit;\n\
             \t\tns_cache_frei = frei;\n\
             \t}\n}\n",
        ),
    ] {
        assert_eq!(t.matches(alt).count(), 1, "llcache.c: Originaltext nicht (genau einmal) gefunden: {alt:?}");
        t = t.replacen(alt, neu, 1);
    }
    let t = format!("/* Erzeugt von esp32/netsurf/build.rs (llcache_kopie) aus content/llcache.c. */\n{t}");
    let datei = ziel.join("llcache.c");
    std::fs::create_dir_all(ziel).expect("Ordner");
    if std::fs::read_to_string(&datei).ok().as_deref() != Some(t.as_str()) {
        std::fs::write(&datei, t).expect("llcache.c");
    }
    datei
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
    ("content", "content.c content_factory.c hlcache.c mimesniff.c textsearch.c urldb.c no_backing_store.c"),
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
         dom_event.c font.c form.c forms.c imagemap.c interaction.c layout.c layout_flex.c object.c redraw.c \
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
