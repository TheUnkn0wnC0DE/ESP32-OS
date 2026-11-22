//! Eingabezustand (Tastatur + Gamepad), plattformunabhängig.
//!
//! „Gedrückt“-Ereignisse werden gelatcht, bis ein Simulationsschritt sie mit
//! [`Input::end_step`] verbraucht – so geht bei hoher Bildrate und festem
//! Physiktakt kein Tastendruck verloren.

/// Unterstützte Tasten.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum Key {
    A,
    B,
    C,
    D,
    E,
    F,
    G,
    H,
    I,
    J,
    K,
    L,
    M,
    N,
    O,
    P,
    Q,
    R,
    S,
    T,
    U,
    V,
    W,
    X,
    Y,
    Z,
    Num0,
    Num1,
    Num2,
    Num3,
    Num4,
    Num5,
    Num6,
    Num7,
    Num8,
    Num9,
    Up,
    Down,
    Left,
    Right,
    Space,
    Enter,
    Escape,
    Tab,
    Backspace,
    Shift,
    Ctrl,
    Alt,
    F1,
    F2,
    F3,
    F4,
    F5,
    F6,
    F7,
    F8,
    F9,
    F10,
    F11,
    F12,
}

impl Key {
    /// Alle Tasten (für Mapping-Tabellen).
    pub const ALL: [Key; 60] = {
        use Key::*;
        [
            A, B, C, D, E, F, G, H, I, J, K, L, M, N, O, P, Q, R, S, T, U, V, W, X, Y, Z, Num0, Num1, Num2, Num3, Num4,
            Num5, Num6, Num7, Num8, Num9, Up, Down, Left, Right, Space, Enter, Escape, Tab, Backspace, Shift, Ctrl,
            Alt, F1, F2, F3, F4, F5, F6, F7, F8, F9, F10, F11, F12,
        ]
    };

    #[inline(always)]
    fn bit(self) -> u64 {
        1u64 << (self as u8)
    }
}

/// Gamepad-Zustand (analoge Achsen in -1..1, Y nach oben positiv).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Gamepad {
    pub connected: bool,
    pub left: (f32, f32),
    pub right: (f32, f32),
    /// Bitmaske der Knöpfe, siehe `PAD_*`-Konstanten.
    pub buttons: u16,
    pub(crate) pressed: u16,
}

pub const PAD_A: u16 = 1;
pub const PAD_B: u16 = 2;
pub const PAD_X: u16 = 4;
pub const PAD_Y: u16 = 8;
pub const PAD_START: u16 = 16;
pub const PAD_BACK: u16 = 32;
pub const PAD_LB: u16 = 64;
pub const PAD_RB: u16 = 128;

impl Gamepad {
    #[inline]
    pub fn down(&self, b: u16) -> bool {
        self.buttons & b != 0
    }
    #[inline]
    pub fn pressed(&self, b: u16) -> bool {
        self.pressed & b != 0
    }
}

/// Tastatur- und Gamepad-Zustand.
#[derive(Clone, Copy, Debug, Default)]
pub struct Input {
    down: u64,
    pressed: u64,
    released: u64,
    pub pad: Gamepad,
}

impl Input {
    pub fn new() -> Self {
        Self::default()
    }

    /// Wird vom Plattform-Layer bei Tastenereignissen aufgerufen.
    pub fn set_key(&mut self, k: Key, is_down: bool) {
        let b = k.bit();
        if is_down {
            if self.down & b == 0 {
                self.pressed |= b;
            }
            self.down |= b;
        } else {
            if self.down & b != 0 {
                self.released |= b;
            }
            self.down &= !b;
        }
    }

    /// Setzt den Gamepad-Zustand (Plattform-Layer, einmal pro Frame).
    pub fn set_pad(&mut self, mut pad: Gamepad) {
        pad.pressed = self.pad.pressed | (pad.buttons & !self.pad.buttons);
        self.pad = pad;
    }

    #[inline]
    pub fn down(&self, k: Key) -> bool {
        self.down & k.bit() != 0
    }
    #[inline]
    pub fn pressed(&self, k: Key) -> bool {
        self.pressed & k.bit() != 0
    }
    #[inline]
    pub fn released(&self, k: Key) -> bool {
        self.released & k.bit() != 0
    }
    /// Irgendeine der Tasten gehalten?
    pub fn any_down(&self, keys: &[Key]) -> bool {
        keys.iter().any(|&k| self.down(k))
    }
    /// Irgendeine der Tasten neu gedrückt?
    pub fn any_pressed(&self, keys: &[Key]) -> bool {
        keys.iter().any(|&k| self.pressed(k))
    }
    /// Achse aus zwei Tasten: -1, 0 oder 1.
    pub fn axis(&self, neg: Key, pos: Key) -> f32 {
        (self.down(pos) as i32 - self.down(neg) as i32) as f32
    }

    /// Verbraucht die gelatchten Ereignisse (nach einem Simulationsschritt).
    pub fn end_step(&mut self) {
        self.pressed = 0;
        self.released = 0;
        self.pad.pressed = 0;
    }

    /// Lässt alle Tasten los (z. B. bei Fokusverlust).
    pub fn release_all(&mut self) {
        self.released |= self.down;
        self.down = 0;
        self.pad = Gamepad::default();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn press_is_latched_until_step() {
        let mut i = Input::new();
        i.set_key(Key::Space, true);
        i.set_key(Key::Space, false);
        assert!(i.pressed(Key::Space) && i.released(Key::Space) && !i.down(Key::Space));
        i.end_step();
        assert!(!i.pressed(Key::Space));
        assert_eq!(Key::ALL.len(), Key::F12 as usize + 1);
    }
}
