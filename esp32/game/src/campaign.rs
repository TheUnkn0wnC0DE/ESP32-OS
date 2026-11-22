//! Kampagne: Schwierigkeitsstufen und die Geschichte zwischen den Kapiteln.
//!
//! Ein Vanillepudding flieht vor der Sommerhitze: durch die Küche in den
//! Kühlschrank, durch den Kühlschrank ins Gefrierfach und von dort hinauf zum
//! Dessert-Buffet, wo die anderen Desserts auf ihn warten.

/// Schwierigkeit einer Partie (wird mit dem Spielstand gespeichert).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Difficulty {
    #[default]
    Easy,
    Normal,
    Hard,
}

impl Difficulty {
    pub const ALL: [Difficulty; 3] = [Difficulty::Easy, Difficulty::Normal, Difficulty::Hard];

    pub fn from_u8(v: u8) -> Self {
        match v {
            1 => Difficulty::Normal,
            2 => Difficulty::Hard,
            _ => Difficulty::Easy,
        }
    }

    pub fn as_u8(self) -> u8 {
        self as u8
    }

    pub fn name(self) -> &'static str {
        match self {
            Difficulty::Easy => "LEICHT",
            Difficulty::Normal => "NORMAL",
            Difficulty::Hard => "SCHWER",
        }
    }

    /// Kurzbeschreibung im Auswahlmenü (zwei Zeilen).
    pub fn info(self) -> [&'static str; 2] {
        match self {
            Difficulty::Easy => ["VIEL ZEIT, LANGSAME GEGNER,", "UNENDLICH VIELE VERSUCHE"],
            Difficulty::Normal => ["DU SCHMILZT SCHNELLER,", "GEGNER UND PLATTFORMEN SIND FLINKER"],
            Difficulty::Hard => ["NUR 3 LEBEN PRO KAPITEL, WENIGER", "CHECKPOINTS, ALLES IST SCHNELL"],
        }
    }

    /// Sekunden, bis der Pudding ohne Abkühlung schmilzt (bzw. einfriert).
    pub fn melt_time(self) -> f32 {
        match self {
            Difficulty::Easy => 80.0,
            Difficulty::Normal => 55.0,
            Difficulty::Hard => 40.0,
        }
    }

    /// Tempo der Gegner (Donuts).
    pub fn enemy_speed(self) -> f32 {
        match self {
            Difficulty::Easy => 1.0,
            Difficulty::Normal => 1.3,
            Difficulty::Hard => 1.6,
        }
    }

    /// Tempo beweglicher Plattformen.
    pub fn platform_speed(self) -> f32 {
        match self {
            Difficulty::Easy => 1.0,
            Difficulty::Normal => 1.15,
            Difficulty::Hard => 1.3,
        }
    }

    /// Tempo der rotierenden Zuckerstangen.
    pub fn spinner_speed(self) -> f32 {
        match self {
            Difficulty::Easy => 1.0,
            Difficulty::Normal => 1.25,
            Difficulty::Hard => 1.5,
        }
    }

    /// Leben pro Kapitel (`None` = unbegrenzt).
    pub fn lives(self) -> Option<u32> {
        match self {
            Difficulty::Hard => Some(3),
            _ => None,
        }
    }

    /// Ist Checkpoint `i` von `n` aktiv? Auf „Schwer“ nur jeder zweite (der
    /// letzte bleibt immer).
    pub fn checkpoint_enabled(self, i: usize, n: usize) -> bool {
        self != Difficulty::Hard || i % 2 == 1 || i + 1 == n
    }
}

/// Eine Seite der Geschichte.
pub struct StoryCard {
    pub title: &'static str,
    pub lines: &'static [&'static str],
}

/// Vor jedem Kapitel eine Karte, am Ende der Abspann.
pub const STORY: [StoryCard; 9] = [
    StoryCard {
        title: "KAPITEL 1: DER KÜCHENTISCH",
        lines: &[
            "SOMMER. DIE SONNE KNALLT DURCHS KÜCHENFENSTER.",
            "AUF DEM TISCH WIRD EIN KLEINER VANILLEPUDDING",
            "IMMER WEICHER UND WACKELIGER ...",
            "",
            "DER KÜHLSCHRANK STEHT GANZ HINTEN IN DER KÜCHE.",
            "ERSTMAL RUNTER VOM TISCH: AB IN DEN BROTKASTEN!",
        ],
    },
    StoryCard {
        title: "KAPITEL 2: DER HERD",
        lines: &[
            "DER BROTKASTEN FÜHRT DIREKT ZUM HERD.",
            "DIE PLATTEN GLÜHEN IM TAKT -",
            "BLOSS NICHT DRAUF STEHEN BLEIBEN!",
            "",
            "DAHINTER WARTET ENDLICH DER KÜHLSCHRANK.",
        ],
    },
    StoryCard {
        title: "KAPITEL 3: IM KÜHLSCHRANK",
        lines: &[
            "GESCHAFFT! DOCH DIE TÜR KLEMMT,",
            "UND WARME LUFT STRÖMT HEREIN.",
            "",
            "KLETTERE ÜBER GLASBÖDEN, BUTTER UND GLÄSER",
            "HINÜBER ZUR KÜHLSCHRANKTÜR!",
        ],
    },
    StoryCard {
        title: "KAPITEL 4: DIE KÜHLSCHRANKTÜR",
        lines: &[
            "IN DEN TÜRFÄCHERN STEHEN FLASCHEN, EIER",
            "UND SENFTUBEN DICHT AN DICHT.",
            "",
            "GANZ OBEN WARTET DIE KLAPPE",
            "ZUM GEFRIERFACH!",
        ],
    },
    StoryCard {
        title: "KAPITEL 5: DAS GEFRIERFACH",
        lines: &[
            "BRRR! HIER IST ES SO KALT, DASS DER PUDDING",
            "LANGSAM EINFRIERT. DAS EIS IST SPIEGELGLATT.",
            "",
            "SCHNAPP DIR WARMEN KAKAO",
            "UND KÄMPF DICH TIEFER INS EIS!",
        ],
    },
    StoryCard {
        title: "KAPITEL 6: DIE EISHÖHLE",
        lines: &[
            "HINTER DEM EIS LIEGT EINE GEHEIME HÖHLE.",
            "MANCHE EISSCHOLLEN SIND BRÜCHIG -",
            "BLEIB NICHT ZU LANGE AUF IHNEN STEHEN!",
            "",
            "EIN LÜFTUNGSSCHACHT FÜHRT NACH OBEN.",
        ],
    },
    StoryCard {
        title: "KAPITEL 7: AUF DEM KÜHLSCHRANK",
        lines: &[
            "DER SCHACHT ENDET OBEN AUF DEM KÜHLSCHRANK.",
            "HIER IST ES WIEDER WARM UND STAUBIG.",
            "",
            "ÜBER MÜSLIPACKUNGEN UND KEKSDOSEN",
            "GEHT ES ZUR GROSSEN PARTY!",
        ],
    },
    StoryCard {
        title: "KAPITEL 8: DAS DESSERT-BUFFET",
        lines: &[
            "GANZ OBEN WARTEN DIE ANDEREN DESSERTS.",
            "SIE FEIERN EINE GROSSE PARTY!",
            "",
            "ABER DER WEG ZUR TORTE IST GEFÄHRLICH:",
            "DONUTS, ZUCKERSTANGEN UND TIEFE SAHNE.",
        ],
    },
    StoryCard {
        title: "GERETTET!",
        lines: &[
            "DER KLEINE PUDDING IST IN SICHERHEIT,",
            "KÜHL, FEST UND WACKELIG WIE NIE.",
            "",
            "DIE DESSERTS FEIERN IHN ALS HELDEN.",
            "",
            "DANKE FÜRS SPIELEN!",
            "EIN SPIEL VON THEUNKN0WNC0DE",
        ],
    },
];

/// Ziel-Text am Kapitelanfang.
pub fn goal_text(chapter: usize) -> &'static str {
    match chapter {
        0 => "AB IN DEN BROTKASTEN!",
        1 => "AB IN DEN KÜHLSCHRANK!",
        2 => "HINÜBER ZUR KÜHLSCHRANKTÜR!",
        3 => "HOCH ZUM GEFRIERFACH!",
        4 => "TIEFER INS EIS!",
        5 => "ZUM LÜFTUNGSSCHACHT!",
        6 => "AB ZUR PARTY!",
        _ => "AB ZUR PARTY-TORTE!",
    }
}