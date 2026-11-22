//! Zeit: fester Simulationstakt und Frame-Statistik (FPS, Frametimes).

/// Fester Simulationstakt („Fix your timestep“) mit Interpolationsfaktor.
#[derive(Clone, Copy, Debug)]
pub struct FixedTimestep {
    /// Schrittweite in Sekunden.
    pub step: f32,
    acc: f32,
    /// Obergrenze pro Frame (verhindert die „Todesspirale“ bei Rucklern).
    pub max_steps: u32,
}

impl FixedTimestep {
    pub fn new(hz: f32) -> Self {
        Self { step: 1.0 / hz, acc: 0.0, max_steps: 8 }
    }

    /// Addiert die vergangene Zeit und liefert die Anzahl fälliger Schritte.
    pub fn advance(&mut self, dt: f32) -> u32 {
        self.acc += dt.clamp(0.0, 0.25);
        let mut n = 0;
        while self.acc >= self.step && n < self.max_steps {
            self.acc -= self.step;
            n += 1;
        }
        if n == self.max_steps && self.acc > self.step {
            self.acc = 0.0;
        }
        n
    }

    /// Anteil des angebrochenen Schritts (0..1) für Render-Interpolation.
    pub fn alpha(&self) -> f32 {
        (self.acc / self.step).clamp(0.0, 1.0)
    }
}

/// Anzahl gespeicherter Frames für Graph und Statistik.
pub const HISTORY: usize = 240;

/// Zusammenfassung der letzten Frames.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct FrameSummary {
    pub fps: f32,
    pub avg_ms: f32,
    pub min_ms: f32,
    pub max_ms: f32,
    /// 99. Perzentil (die langsamsten 1 %).
    pub p99_ms: f32,
    /// Reine CPU-Zeit (Update + Render + Present ohne Warten auf VSync).
    pub cpu_ms: f32,
    pub update_ms: f32,
    pub render_ms: f32,
    pub present_ms: f32,
}

/// Ringpuffer der Frame-Zeiten. Die Zusammenfassung wird nur 4× pro Sekunde
/// neu berechnet (ruhige Anzeige, praktisch keine Kosten).
#[derive(Clone, Debug)]
pub struct FrameStats {
    frame_ms: [f32; HISTORY],
    cpu_ms: [f32; HISTORY],
    head: usize,
    count: usize,
    since_summary: f32,
    acc: [f32; 4],
    acc_frames: u32,
    summary: FrameSummary,
}

impl Default for FrameStats {
    fn default() -> Self {
        Self::new()
    }
}

impl FrameStats {
    pub fn new() -> Self {
        Self {
            frame_ms: [0.0; HISTORY],
            cpu_ms: [0.0; HISTORY],
            head: 0,
            count: 0,
            since_summary: 0.0,
            acc: [0.0; 4],
            acc_frames: 0,
            summary: FrameSummary::default(),
        }
    }

    /// Erfasst einen Frame. Alle Zeiten in Millisekunden.
    pub fn record(&mut self, frame_ms: f32, update_ms: f32, render_ms: f32, present_ms: f32) {
        let cpu = update_ms + render_ms + present_ms;
        self.frame_ms[self.head] = frame_ms;
        self.cpu_ms[self.head] = cpu;
        self.head = (self.head + 1) % HISTORY;
        self.count = (self.count + 1).min(HISTORY);
        self.acc[0] += update_ms;
        self.acc[1] += render_ms;
        self.acc[2] += present_ms;
        self.acc[3] += frame_ms;
        self.acc_frames += 1;
        self.since_summary += frame_ms;
        if self.since_summary >= 250.0 || self.summary.fps == 0.0 {
            self.refresh_summary();
        }
    }

    /// Berechnet die Zusammenfassung sofort neu (z. B. am Ende eines Benchmarks).
    pub fn refresh_summary(&mut self) {
        let n = self.count;
        if n == 0 || self.acc_frames == 0 {
            return;
        }
        let mut sorted = [0.0f32; HISTORY];
        sorted[..n].copy_from_slice(&self.ordered_slice_copy()[..n]);
        let s = &mut sorted[..n];
        s.sort_unstable_by(f32::total_cmp);
        let f = self.acc_frames.max(1) as f32;
        let avg = self.acc[3] / f;
        self.summary = FrameSummary {
            fps: if avg > 0.0 { 1000.0 / avg } else { 0.0 },
            avg_ms: avg,
            min_ms: s[0],
            max_ms: s[n - 1],
            p99_ms: s[((n - 1) as f32 * 0.99) as usize],
            cpu_ms: (self.acc[0] + self.acc[1] + self.acc[2]) / f,
            update_ms: self.acc[0] / f,
            render_ms: self.acc[1] / f,
            present_ms: self.acc[2] / f,
        };
        self.acc = [0.0; 4];
        self.acc_frames = 0;
        self.since_summary = 0.0;
    }

    fn ordered_slice_copy(&self) -> [f32; HISTORY] {
        let mut out = [0.0; HISTORY];
        for (i, v) in self.history().enumerate() {
            out[i] = v;
        }
        out
    }

    /// Letzte Zusammenfassung.
    pub fn summary(&self) -> FrameSummary {
        self.summary
    }

    /// Frame-Zeiten vom ältesten zum neuesten.
    pub fn history(&self) -> impl Iterator<Item = f32> + '_ {
        let start = (self.head + HISTORY - self.count) % HISTORY;
        (0..self.count).map(move |i| self.frame_ms[(start + i) % HISTORY])
    }

    /// CPU-Zeiten vom ältesten zum neuesten.
    pub fn cpu_history(&self) -> impl Iterator<Item = f32> + '_ {
        let start = (self.head + HISTORY - self.count) % HISTORY;
        (0..self.count).map(move |i| self.cpu_ms[(start + i) % HISTORY])
    }

    pub fn len(&self) -> usize {
        self.count
    }

    pub fn is_empty(&self) -> bool {
        self.count == 0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fixed_timestep_counts_steps() {
        let mut t = FixedTimestep::new(100.0);
        assert_eq!(t.advance(0.025), 2);
        assert!((t.alpha() - 0.5).abs() < 1e-3);
    }

    #[test]
    fn stats_summarize() {
        let mut s = FrameStats::new();
        for i in 0..100 {
            s.record(if i == 50 { 10.0 } else { 2.0 }, 0.5, 1.0, 0.25);
        }
        s.refresh_summary();
        let sum = s.summary();
        assert!(sum.max_ms >= 10.0 && sum.min_ms <= 2.0);
        assert!((sum.cpu_ms - 1.75).abs() < 1e-3);
    }
}
