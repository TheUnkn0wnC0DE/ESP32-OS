//! Laufzeit-Erkennung von CPU-Fähigkeiten – ohne `std` (CPUID/XGETBV).
//!
//! Der Kern läuft überall mit SSE2 (x86_64-Basis) bzw. skalar; schnellere
//! Pfade (AVX2) werden nur genutzt, wenn CPU *und* Betriebssystem sie
//! unterstützen. Das Ergebnis wird einmalig ermittelt und zwischengespeichert.

#[cfg(target_arch = "x86_64")]
use core::sync::atomic::{AtomicU8, Ordering};

#[cfg(target_arch = "x86_64")]
static AVX2: AtomicU8 = AtomicU8::new(0); // 0 = unbekannt, 1 = nein, 2 = ja
#[cfg(target_arch = "x86_64")]
static V3: AtomicU8 = AtomicU8::new(0);

/// x86-64-v3-Funktionen ohne FMA (AVX2, BMI1/2, LZCNT, POPCNT) – Grundlage
/// für die mehrfach kompilierten Render-Pfade (Multiversioning).
#[inline]
pub fn has_v3() -> bool {
    #[cfg(target_arch = "x86_64")]
    {
        match V3.load(Ordering::Relaxed) {
            1 => false,
            2 => true,
            _ => {
                let yes = detect_avx2() && detect_bmi_lzcnt_popcnt();
                V3.store(if yes { 2 } else { 1 }, Ordering::Relaxed);
                yes
            }
        }
    }
    #[cfg(not(target_arch = "x86_64"))]
    {
        false
    }
}

#[cfg(target_arch = "x86_64")]
fn detect_bmi_lzcnt_popcnt() -> bool {
    use core::arch::x86_64::{__cpuid, __cpuid_count};
    let popcnt = __cpuid(1).ecx & (1 << 23) != 0;
    let l7 = __cpuid_count(7, 0);
    let bmi1 = l7.ebx & (1 << 3) != 0;
    let bmi2 = l7.ebx & (1 << 8) != 0;
    let lzcnt = __cpuid(0x8000_0000).eax >= 0x8000_0001 && __cpuid(0x8000_0001).ecx & (1 << 5) != 0;
    popcnt && bmi1 && bmi2 && lzcnt
}

/// Unterstützt diese CPU (und das OS) AVX2?
#[inline]
pub fn has_avx2() -> bool {
    #[cfg(target_arch = "x86_64")]
    {
        match AVX2.load(Ordering::Relaxed) {
            1 => false,
            2 => true,
            _ => {
                let yes = detect_avx2();
                AVX2.store(if yes { 2 } else { 1 }, Ordering::Relaxed);
                yes
            }
        }
    }
    #[cfg(not(target_arch = "x86_64"))]
    {
        false
    }
}

/// Erzwingt die Basis-Pfade (SSE2), z. B. für Vergleichsmessungen oder um
/// die Gleichheit beider Pfade zu prüfen. `true` = AVX2/v3 aus.
pub fn force_baseline(off: bool) {
    #[cfg(target_arch = "x86_64")]
    {
        let a = if off {
            1
        } else if detect_avx2() {
            2
        } else {
            1
        };
        let v = if off {
            1
        } else if detect_avx2() && detect_bmi_lzcnt_popcnt() {
            2
        } else {
            1
        };
        AVX2.store(a, Ordering::Relaxed);
        V3.store(v, Ordering::Relaxed);
    }
    #[cfg(not(target_arch = "x86_64"))]
    let _ = off;
}

#[cfg(target_arch = "x86_64")]
fn detect_avx2() -> bool {
    use core::arch::x86_64::{__cpuid, __cpuid_count};
    // CPUID ist auf x86_64 immer verfügbar (sichere Funktion).
    let l1 = __cpuid(1);
    let osxsave = l1.ecx & (1 << 27) != 0;
    let avx = l1.ecx & (1 << 28) != 0;
    if !(osxsave && avx) {
        return false;
    }
    // SAFETY: OSXSAVE gesetzt → XGETBV ist erlaubt.
    let xcr0 = unsafe { xgetbv0() };
    if xcr0 & 0b110 != 0b110 {
        return false; // OS sichert die YMM-Register nicht
    }
    // CPUID Leaf 7 existiert auf allen AVX-fähigen CPUs.
    let l7 = __cpuid_count(7, 0);
    l7.ebx & (1 << 5) != 0
}

#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "xsave")]
unsafe fn xgetbv0() -> u64 {
    // SAFETY: Aufrufer hat OSXSAVE geprüft.
    unsafe { core::arch::x86_64::_xgetbv(0) }
}
