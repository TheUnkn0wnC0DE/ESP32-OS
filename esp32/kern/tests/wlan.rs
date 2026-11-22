//! WLAN des Kerns (Block D, W3/W4) mit dem simulierten Treiber: Suchen,
//! Verbinden, Merken nach Erfolg, falsches Passwort, automatisches
//! Wiederverbinden mit dem stärksten bekannten Netz, „Trennen“ von Hand.
//! Ein Test, weil Treiber und Zustand prozessweit gelten.

use kern::wlan::{self, Ereignis, Netz, Sicherheit, Simulation};

fn netz(ssid: &str, rssi: i32, s: Sicherheit, ax: bool) -> Netz {
    Netz { ssid: ssid.into(), rssi, kanal: 6, sicherheit: s, ax }
}

#[test]
fn wlan_ablauf() {
    // Kein Treiber: nichts vorhanden, Zustand leer
    assert!(!wlan::vorhanden());
    assert_eq!(wlan::zustand(), wlan::Zustand::default());
    assert!(wlan::suchen().is_err());

    let sim: &'static Simulation = Box::leak(Box::new(
        Simulation::neu(vec![
            (netz("Wohnung", -48, Sicherheit::Wpa2Wpa3, true), "geheim-123".into()),
            (netz("Nachbar", -70, Sicherheit::Wpa2, false), "x".into()),
            (netz("Nachbar", -80, Sicherheit::Wpa2, false), "x".into()),
            (netz("", -60, Sicherheit::Wpa2, false), "".into()),
            (netz("Cafe", -82, Sicherheit::Offen, false), "".into()),
            (netz("Firma", -50, Sicherheit::Firma, false), "".into()),
        ])
        // nach einem Neustart: ein gemerktes Netz, das gerade nicht in Reichweite ist
        .mit_gespeicherten(vec![("Ferienhaus".into(), "alt".into())]),
    ));
    struct Weiter(&'static Simulation);
    impl wlan::Treiber for Weiter {
        fn zustand(&self) -> wlan::Zustand {
            self.0.zustand()
        }
        fn suchen(&self) -> Result<Vec<Netz>, i32> {
            self.0.suchen()
        }
        fn verbinden(&self, s: &str, p: &str) -> Result<(), i32> {
            self.0.verbinden(s, p)
        }
        fn trennen(&self) {
            self.0.trennen()
        }
        fn laden(&self) -> Vec<(String, String)> {
            self.0.laden()
        }
        fn sichern(&self, n: &[(String, String)]) -> bool {
            self.0.sichern(n)
        }
    }
    use wlan::Treiber;
    assert!(wlan::treiber_setzen(Box::new(Weiter(sim))));
    assert!(!wlan::treiber_setzen(Box::new(Weiter(sim))), "nur einmal");
    assert!(wlan::vorhanden());

    // Suchen: sortiert, doppelte SSID nur einmal (stärkster), leere SSID weg
    let n = wlan::suchen().unwrap();
    let namen: Vec<&str> = n.iter().map(|x| x.ssid.as_str()).collect();
    assert_eq!(namen, ["Wohnung", "Firma", "Nachbar", "Cafe"]);
    assert_eq!(n[2].rssi, -70);
    assert_eq!(wlan::balken(-48), 4);
    assert_eq!(wlan::balken(-70), 2);
    assert_eq!(wlan::balken(-90), 0);
    assert!(Sicherheit::Wpa2Wpa3.unterstuetzt() && !Sicherheit::Firma.unterstuetzt());
    assert!(!Sicherheit::Wep.unterstuetzt() && !Sicherheit::Wpa.unterstuetzt(), "unsicher");
    // Ohne Testnetz: echte Namen
    assert_eq!(wlan::anzeigename("Nachbar"), "Nachbar");
    assert_eq!(wlan::test_passwort("Wohnung"), None);

    // Wache ohne bekanntes Netz in Reichweite: ein Suchlauf, dann Pause
    let mut t = 1_000u64;
    let e = wlan::wache(t);
    assert_eq!(e, [Ereignis::KeinBekanntesNetz]);
    let suchen_vorher = sim.suchen_zahl();
    t += 5_000;
    assert!(wlan::wache(t).is_empty(), "Pause nach dem Versuch");
    assert_eq!(sim.suchen_zahl(), suchen_vorher, "in der Pause wird nicht gesucht");

    // Falsches Passwort: nicht verbunden, Grund, nicht gemerkt
    wlan::verbinden("Wohnung", "falsch", true).unwrap();
    assert!(wlan::zustand().verbindet);
    t += 500;
    let e = wlan::wache(t);
    assert!(e.is_empty(), "{e:?}");
    let z = wlan::zustand();
    assert!(!z.verbunden && !z.verbindet);
    assert_eq!(wlan::grund_text(z.grund), "Wrong password?");
    assert_eq!(wlan::bekannte(), ["Ferienhaus"]);

    // Richtiges Passwort: verbunden (HE), danach gemerkt, vorne in der Liste
    wlan::verbinden("Wohnung", "geheim-123", true).unwrap();
    t += 500;
    let e = wlan::wache(t);
    assert_eq!(e, [Ereignis::Verbunden { ip: [192, 168, 1, 50], rssi: -48, kanal: 6, he: true }]);
    assert!(wlan::zustand().verbunden && wlan::zustand().he);
    assert_eq!(wlan::verbunden_mit().as_deref(), Some("Wohnung"));
    assert_eq!(wlan::bekannte(), ["Wohnung", "Ferienhaus"]);
    assert_eq!(sim.laden()[0], ("Wohnung".to_string(), "geheim-123".to_string()), "im Treiber gesichert");
    assert_eq!(wlan::passwort_von("Wohnung").as_deref(), Some("geheim-123"));

    // Verbunden: Wache sucht nicht
    let s0 = sim.suchen_zahl();
    t += 60_000;
    assert!(wlan::wache(t).is_empty());
    assert_eq!(sim.suchen_zahl(), s0);

    // Verbindung bricht ab (Zugangspunkt weg) → Getrennt, danach automatisch wieder
    sim.abbrechen(200);
    t += 500;
    let e = wlan::wache(t);
    assert_eq!(e, [Ereignis::Getrennt { grund: 200 }, Ereignis::Versuch { nr: 0 }]);
    let geschrieben = sim.sichern_zahl();
    t += 500;
    let e = wlan::wache(t);
    assert!(matches!(e[..], [Ereignis::Verbunden { .. }]), "{e:?}");
    assert_eq!(sim.sichern_zahl(), geschrieben, "unverändert: nicht neu schreiben");

    // Trennen von Hand: kein automatisches Wiederverbinden
    wlan::trennen();
    assert!(!wlan::zustand().verbunden);
    assert_eq!(wlan::verbunden_mit(), None);
    for _ in 0..5 {
        t += 400_000;
        assert!(wlan::wache(t).iter().all(|e| matches!(e, Ereignis::Getrennt { .. })));
    }
    assert!(!wlan::zustand().verbunden);

    // Offenes Netz von Hand (nicht merken), dann vergessen
    wlan::verbinden("Cafe", "", false).unwrap();
    t += 500;
    assert!(matches!(wlan::wache(t)[..], [Ereignis::Verbunden { .. }]));
    assert_eq!(wlan::bekannte(), ["Wohnung", "Ferienhaus"], "nicht gemerkt");
    assert!(wlan::vergessen("Ferienhaus"));
    assert_eq!(wlan::bekannte(), ["Wohnung"]);
    // gesichert wird erst in wache (Firmware-Faden mit internem Stapel)
    assert_eq!(sim.laden().len(), 2);
    t += 500;
    wlan::wache(t);
    assert_eq!(sim.laden().len(), 1);

    // Test-Build: Testnetz oben, Namen anonymisiert, Passwort vorbelegt
    wlan::test_netz_setzen("Cafe", "");
    let n = wlan::suchen().unwrap();
    assert_eq!(n[0].ssid, "Cafe");
    assert_eq!(wlan::anzeigename("Cafe"), "Test network (wlan.local)");
    assert_eq!(wlan::anzeigename("Wohnung"), "Network 1");
    assert_eq!(wlan::anzeigename("Nachbar"), "Network 2");
    assert_eq!(wlan::anzeigename("Wohnung"), "Network 1", "gleichbleibend");
    assert_eq!(wlan::test_passwort("Cafe").as_deref(), Some(""));
    assert_eq!(wlan::test_passwort("Wohnung"), None);

    // Höchstens MAX_BEKANNT gemerkte Netze
    for i in 0..12 {
        wlan::merken(&format!("Netz{i}"), "p");
    }
    let b = wlan::bekannte();
    assert_eq!(b.len(), wlan::MAX_BEKANNT);
    assert_eq!(b[0], "Netz11");
}
