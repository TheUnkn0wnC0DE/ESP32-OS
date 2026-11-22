//! Win32-Backend über direktes FFI (user32/gdi32/kernel32/winmm), ohne Crates.
//!
//! * Darstellung per `StretchDIBits` (GDI skaliert Nearest-Neighbour hoch).
//! * VSync über `DwmFlush` (dynamisch geladen, Fallback: Schlaf-Limiter).
//! * Gamepad über XInput (dynamisch geladen; fehlt es, gibt es kein Pad).
//! * Tastatur nur über Fenstermeldungen → keine globalen Tastenabfragen.

#![allow(non_snake_case, clippy::upper_case_acronyms, non_camel_case_types)]

use super::Platform;
use crate::input::{Gamepad, Input, Key, PAD_A, PAD_B, PAD_BACK, PAD_LB, PAD_RB, PAD_START, PAD_X, PAD_Y};
use crate::render::Framebuffer;
use core::ffi::c_void;
use core::ptr::{null, null_mut};
use std::string::String;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};
use std::vec::Vec;

type HWND = *mut c_void;
type HINSTANCE = *mut c_void;
type HDC = *mut c_void;
type HMODULE = *mut c_void;
type HMONITOR = *mut c_void;
type WPARAM = usize;
type LPARAM = isize;
type LRESULT = isize;
type BOOL = i32;
type WNDPROC = unsafe extern "system" fn(HWND, u32, WPARAM, LPARAM) -> LRESULT;

#[repr(C)]
struct WNDCLASSEXW {
    cbSize: u32,
    style: u32,
    lpfnWndProc: Option<WNDPROC>,
    cbClsExtra: i32,
    cbWndExtra: i32,
    hInstance: HINSTANCE,
    hIcon: *mut c_void,
    hCursor: *mut c_void,
    hbrBackground: *mut c_void,
    lpszMenuName: *const u16,
    lpszClassName: *const u16,
    hIconSm: *mut c_void,
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
struct POINT {
    x: i32,
    y: i32,
}

#[repr(C)]
struct MSG {
    hwnd: HWND,
    message: u32,
    wParam: WPARAM,
    lParam: LPARAM,
    time: u32,
    pt: POINT,
    lPrivate: u32,
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
struct RECT {
    left: i32,
    top: i32,
    right: i32,
    bottom: i32,
}

#[repr(C)]
#[derive(Clone, Copy)]
struct BITMAPINFOHEADER {
    biSize: u32,
    biWidth: i32,
    biHeight: i32,
    biPlanes: u16,
    biBitCount: u16,
    biCompression: u32,
    biSizeImage: u32,
    biXPelsPerMeter: i32,
    biYPelsPerMeter: i32,
    biClrUsed: u32,
    biClrImportant: u32,
}

#[repr(C)]
#[derive(Clone, Copy)]
struct BITMAPINFO {
    bmiHeader: BITMAPINFOHEADER,
    bmiColors: [u32; 1],
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
struct MONITORINFO {
    cbSize: u32,
    rcMonitor: RECT,
    rcWork: RECT,
    dwFlags: u32,
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
struct WINDOWPLACEMENT {
    length: u32,
    flags: u32,
    showCmd: u32,
    ptMinPosition: POINT,
    ptMaxPosition: POINT,
    rcNormalPosition: RECT,
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
struct XINPUT_GAMEPAD {
    wButtons: u16,
    bLeftTrigger: u8,
    bRightTrigger: u8,
    sThumbLX: i16,
    sThumbLY: i16,
    sThumbRX: i16,
    sThumbRY: i16,
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
struct XINPUT_STATE {
    dwPacketNumber: u32,
    Gamepad: XINPUT_GAMEPAD,
}

type DwmFlushFn = unsafe extern "system" fn() -> i32;
type XInputGetStateFn = unsafe extern "system" fn(u32, *mut XINPUT_STATE) -> u32;

#[link(name = "user32")]
unsafe extern "system" {
    fn RegisterClassExW(lpwcx: *const WNDCLASSEXW) -> u16;
    fn CreateWindowExW(
        ex_style: u32,
        class: *const u16,
        title: *const u16,
        style: u32,
        x: i32,
        y: i32,
        w: i32,
        h: i32,
        parent: HWND,
        menu: *mut c_void,
        inst: HINSTANCE,
        param: *mut c_void,
    ) -> HWND;
    fn DefWindowProcW(hwnd: HWND, msg: u32, w: WPARAM, l: LPARAM) -> LRESULT;
    fn DestroyWindow(hwnd: HWND) -> BOOL;
    fn PeekMessageW(msg: *mut MSG, hwnd: HWND, min: u32, max: u32, remove: u32) -> BOOL;
    fn DispatchMessageW(msg: *const MSG) -> LRESULT;
    fn GetDC(hwnd: HWND) -> HDC;
    fn ReleaseDC(hwnd: HWND, hdc: HDC) -> i32;
    fn GetClientRect(hwnd: HWND, rect: *mut RECT) -> BOOL;
    fn AdjustWindowRect(rect: *mut RECT, style: u32, menu: BOOL) -> BOOL;
    fn LoadCursorW(inst: HINSTANCE, name: *const u16) -> *mut c_void;
    fn SetWindowTextW(hwnd: HWND, text: *const u16) -> BOOL;
    fn GetSystemMetrics(index: i32) -> i32;
    fn SetProcessDPIAware() -> BOOL;
    fn GetWindowLongPtrW(hwnd: HWND, index: i32) -> isize;
    fn SetWindowLongPtrW(hwnd: HWND, index: i32, value: isize) -> isize;
    fn SetWindowPos(hwnd: HWND, after: HWND, x: i32, y: i32, cx: i32, cy: i32, flags: u32) -> BOOL;
    fn MonitorFromWindow(hwnd: HWND, flags: u32) -> HMONITOR;
    fn GetMonitorInfoW(mon: HMONITOR, mi: *mut MONITORINFO) -> BOOL;
    fn GetWindowPlacement(hwnd: HWND, wp: *mut WINDOWPLACEMENT) -> BOOL;
    fn SetWindowPlacement(hwnd: HWND, wp: *const WINDOWPLACEMENT) -> BOOL;
    fn ValidateRect(hwnd: HWND, rect: *const RECT) -> BOOL;
}

#[link(name = "gdi32")]
unsafe extern "system" {
    fn StretchDIBits(
        hdc: HDC,
        xd: i32,
        yd: i32,
        wd: i32,
        hd: i32,
        xs: i32,
        ys: i32,
        ws: i32,
        hs: i32,
        bits: *const c_void,
        bmi: *const BITMAPINFO,
        usage: u32,
        rop: u32,
    ) -> i32;
    fn SetStretchBltMode(hdc: HDC, mode: i32) -> i32;
    fn PatBlt(hdc: HDC, x: i32, y: i32, w: i32, h: i32, rop: u32) -> BOOL;
    fn CreateCompatibleDC(hdc: HDC) -> HDC;
    fn CreateDIBSection(
        hdc: HDC,
        bmi: *const BITMAPINFO,
        usage: u32,
        bits: *mut *mut c_void,
        section: *mut c_void,
        offset: u32,
    ) -> *mut c_void;
    fn SelectObject(hdc: HDC, obj: *mut c_void) -> *mut c_void;
    fn DeleteObject(obj: *mut c_void) -> BOOL;
    fn DeleteDC(hdc: HDC) -> BOOL;
    fn GdiFlush() -> BOOL;
}

#[link(name = "kernel32")]
unsafe extern "system" {
    fn GetModuleHandleW(name: *const u16) -> HMODULE;
    fn LoadLibraryW(name: *const u16) -> HMODULE;
    fn GetProcAddress(module: HMODULE, name: *const u8) -> *mut c_void;
    fn GetConsoleProcessList(list: *mut u32, count: u32) -> u32;
    fn FreeConsole() -> BOOL;
    fn GetCurrentThread() -> *mut c_void;
    fn QueryThreadCycleTime(thread: *mut c_void, cycles: *mut u64) -> BOOL;
    fn SetThreadAffinityMask(thread: *mut c_void, mask: usize) -> usize;
}

/// Bindet den aktuellen Thread an einen logischen Prozessor (für
/// reproduzierbare Benchmarks auf CPUs mit P- und E-Kernen).
pub fn pin_current_thread(cpu: usize) -> bool {
    if cpu >= usize::BITS as usize {
        return false;
    }
    // SAFETY: Pseudo-Handle des eigenen Threads, gültige Maske.
    unsafe { SetThreadAffinityMask(GetCurrentThread(), 1usize << cpu) != 0 }
}

/// Vom aktuellen Thread verbrauchte CPU-Zyklen (ohne Zeiten, in denen er
/// verdrängt war) – stabile Grundlage für Benchmarks auf belebten Rechnern.
pub fn thread_cycles() -> u64 {
    let mut c = 0u64;
    // SAFETY: Pseudo-Handle des eigenen Threads, gültiger Ausgabezeiger.
    unsafe { QueryThreadCycleTime(GetCurrentThread(), &mut c) };
    c
}

#[repr(C)]
#[derive(Default)]
struct PROCESS_MEMORY_COUNTERS {
    cb: u32,
    PageFaultCount: u32,
    PeakWorkingSetSize: usize,
    WorkingSetSize: usize,
    QuotaPeakPagedPoolUsage: usize,
    QuotaPagedPoolUsage: usize,
    QuotaPeakNonPagedPoolUsage: usize,
    QuotaNonPagedPoolUsage: usize,
    PagefileUsage: usize,
    PeakPagefileUsage: usize,
}

type GetProcessMemoryInfoFn = unsafe extern "system" fn(*mut c_void, *mut PROCESS_MEMORY_COUNTERS, u32) -> BOOL;

/// (Spitzen-Arbeitsspeicher, privater Speicher) des Prozesses in Bytes.
pub fn process_memory() -> Option<(usize, usize)> {
    let k32 = wide("kernel32.dll");
    // SAFETY: dynamisch geladene, dokumentierte Funktion mit passender Signatur;
    // Pseudo-Handle -1 = eigener Prozess; Struktur mit korrektem `cb`.
    unsafe {
        let m = GetModuleHandleW(k32.as_ptr());
        let p = GetProcAddress(m, c"K32GetProcessMemoryInfo".as_ptr().cast());
        if p.is_null() {
            return None;
        }
        let f = core::mem::transmute::<*mut c_void, GetProcessMemoryInfoFn>(p);
        let mut pmc = PROCESS_MEMORY_COUNTERS {
            cb: core::mem::size_of::<PROCESS_MEMORY_COUNTERS>() as u32,
            ..Default::default()
        };
        if f(-1isize as *mut c_void, &mut pmc, pmc.cb) == 0 {
            return None;
        }
        Some((pmc.PeakWorkingSetSize, pmc.PagefileUsage))
    }
}

/// Schließt das Konsolenfenster, falls es nur für dieses Programm erzeugt
/// wurde (Start per Doppelklick). Aus einem Terminal gestartet bleibt die
/// Konsole erhalten.
pub fn detach_own_console() {
    let mut ids = [0u32; 4];
    // SAFETY: Puffer mit 4 Einträgen; FreeConsole löst nur unsere Bindung.
    unsafe {
        if GetConsoleProcessList(ids.as_mut_ptr(), ids.len() as u32) == 1 {
            FreeConsole();
        }
    }
}

#[link(name = "winmm")]
unsafe extern "system" {
    fn timeBeginPeriod(period: u32) -> u32;
    fn timeEndPeriod(period: u32) -> u32;
}

const CS_VREDRAW: u32 = 0x0001;
const CS_HREDRAW: u32 = 0x0002;
const CS_OWNDC: u32 = 0x0020;
const WS_OVERLAPPEDWINDOW: u32 = 0x00CF_0000;
const WS_VISIBLE: u32 = 0x1000_0000;
const WM_DESTROY: u32 = 0x0002;
const WM_PAINT: u32 = 0x000F;
const WM_CLOSE: u32 = 0x0010;
const WM_QUIT: u32 = 0x0012;
const WM_ERASEBKGND: u32 = 0x0014;
const WM_KILLFOCUS: u32 = 0x0008;
const WM_KEYDOWN: u32 = 0x0100;
const WM_KEYUP: u32 = 0x0101;
const WM_SYSKEYDOWN: u32 = 0x0104;
const WM_SYSKEYUP: u32 = 0x0105;
const PM_REMOVE: u32 = 0x0001;
const SM_CXSCREEN: i32 = 0;
const SM_CYSCREEN: i32 = 1;
const COLORONCOLOR: i32 = 3;
const DIB_RGB_COLORS: u32 = 0;
const SRCCOPY: u32 = 0x00CC_0020;
const BLACKNESS: u32 = 0x0000_0042;
const GWL_STYLE: i32 = -16;
const SWP_NOSIZE: u32 = 0x0001;
const SWP_NOMOVE: u32 = 0x0002;
const SWP_NOZORDER: u32 = 0x0004;
const SWP_FRAMECHANGED: u32 = 0x0020;
const SWP_NOOWNERZORDER: u32 = 0x0200;
const MONITOR_DEFAULTTOPRIMARY: u32 = 1;
const IDC_ARROW: usize = 32512;
const ERROR_SUCCESS: u32 = 0;

static CLOSE_REQUESTED: AtomicBool = AtomicBool::new(false);
static FOCUS_LOST: AtomicBool = AtomicBool::new(false);

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(core::iter::once(0)).collect()
}

unsafe extern "system" fn wnd_proc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    match msg {
        WM_CLOSE => {
            CLOSE_REQUESTED.store(true, Ordering::Relaxed);
            0
        }
        WM_DESTROY => 0,
        WM_KILLFOCUS => {
            FOCUS_LOST.store(true, Ordering::Relaxed);
            0
        }
        // Wir zeichnen selbst jeden Frame → kein Löschen/Flackern.
        WM_ERASEBKGND => 1,
        WM_PAINT => {
            // SAFETY: gültiges Fensterhandle aus dem Aufruf.
            unsafe { ValidateRect(hwnd, null()) };
            0
        }
        // SAFETY: Standardbehandlung mit unveränderten Parametern.
        _ => unsafe { DefWindowProcW(hwnd, msg, wp, lp) },
    }
}

fn bitmap_info(w: i32, h: i32) -> BITMAPINFO {
    BITMAPINFO {
        bmiHeader: BITMAPINFOHEADER {
            biSize: core::mem::size_of::<BITMAPINFOHEADER>() as u32,
            biWidth: w,
            biHeight: -h, // negativ = von oben nach unten
            biPlanes: 1,
            biBitCount: 32,
            biCompression: 0, // BI_RGB
            biSizeImage: 0,
            biXPelsPerMeter: 0,
            biYPelsPerMeter: 0,
            biClrUsed: 0,
            biClrImportant: 0,
        },
        bmiColors: [0],
    }
}

fn map_vk(vk: usize) -> Option<Key> {
    Some(match vk {
        0x41..=0x5A => Key::ALL[vk - 0x41],
        0x30..=0x39 => Key::ALL[26 + vk - 0x30],
        0x70..=0x7B => Key::ALL[48 + vk - 0x70],
        0x26 => Key::Up,
        0x28 => Key::Down,
        0x25 => Key::Left,
        0x27 => Key::Right,
        0x20 => Key::Space,
        0x0D => Key::Enter,
        0x1B => Key::Escape,
        0x09 => Key::Tab,
        0x08 => Key::Backspace,
        0x10 => Key::Shift,
        0x11 => Key::Ctrl,
        0x12 => Key::Alt,
        _ => return None,
    })
}

/// Registriert die Fensterklasse (idempotent).
fn register_class(class: &[u16]) -> HINSTANCE {
    // SAFETY: Alle Zeiger zeigen auf gültige, lebende Daten; die Klasse
    // wird höchstens doppelt registriert (Fehler wird dann ignoriert).
    unsafe {
        let hinst = GetModuleHandleW(null());
        let wc = WNDCLASSEXW {
            cbSize: core::mem::size_of::<WNDCLASSEXW>() as u32,
            style: CS_OWNDC | CS_HREDRAW | CS_VREDRAW,
            lpfnWndProc: Some(wnd_proc),
            cbClsExtra: 0,
            cbWndExtra: 0,
            hInstance: hinst,
            hIcon: null_mut(),
            hCursor: LoadCursorW(null_mut(), IDC_ARROW as *const u16),
            hbrBackground: null_mut(),
            lpszMenuName: null(),
            lpszClassName: class.as_ptr(),
            hIconSm: null_mut(),
        };
        RegisterClassExW(&wc);
        hinst
    }
}

/// Ein Spielfenster.
pub struct Win32Window {
    hwnd: HWND,
    hdc: HDC,
    bmi: BITMAPINFO,
    style: u32,
    fullscreen: bool,
    placement: WINDOWPLACEMENT,
    dwm_flush: Option<DwmFlushFn>,
    xinput: Option<XInputGetStateFn>,
    pad_retry: Instant,
    pad_connected: bool,
    last_title: Instant,
}

impl Win32Window {
    /// Öffnet ein Fenster für einen Framebuffer der Größe `fb_w`×`fb_h`.
    /// `scale = None` wählt den größten ganzzahligen Faktor, der auf den
    /// Bildschirm passt.
    pub fn new(title: &str, fb_w: usize, fb_h: usize, scale: Option<u32>) -> Result<Self, String> {
        let class = wide("SpatzEngineWindow");
        let title_w = wide(title);
        // SAFETY: FFI-Aufrufe mit gültigen Argumenten; Rückgaben werden geprüft.
        unsafe {
            SetProcessDPIAware();
            let hinst = register_class(&class);
            let sw = GetSystemMetrics(SM_CXSCREEN).max(320);
            let sh = GetSystemMetrics(SM_CYSCREEN).max(240);
            let s = scale.map(|s| s.max(1) as i32).unwrap_or_else(|| {
                ((sw as f32 * 0.85 / fb_w as f32).min(sh as f32 * 0.85 / fb_h as f32) as i32).max(1)
            });
            let style = WS_OVERLAPPEDWINDOW;
            let mut r = RECT { left: 0, top: 0, right: fb_w as i32 * s, bottom: fb_h as i32 * s };
            AdjustWindowRect(&mut r, style, 0);
            let (ww, wh) = (r.right - r.left, r.bottom - r.top);
            let hwnd = CreateWindowExW(
                0,
                class.as_ptr(),
                title_w.as_ptr(),
                style | WS_VISIBLE,
                ((sw - ww) / 2).max(0),
                ((sh - wh) / 2).max(0),
                ww,
                wh,
                null_mut(),
                null_mut(),
                hinst,
                null_mut(),
            );
            if hwnd.is_null() {
                return Err("CreateWindowExW fehlgeschlagen".into());
            }
            let hdc = GetDC(hwnd);
            SetStretchBltMode(hdc, COLORONCOLOR);
            timeBeginPeriod(1);
            CLOSE_REQUESTED.store(false, Ordering::Relaxed);
            Ok(Self {
                hwnd,
                hdc,
                bmi: bitmap_info(fb_w as i32, fb_h as i32),
                style,
                fullscreen: false,
                placement: WINDOWPLACEMENT {
                    length: core::mem::size_of::<WINDOWPLACEMENT>() as u32,
                    ..Default::default()
                },
                dwm_flush: load_dwm_flush(),
                xinput: load_xinput(),
                pad_retry: Instant::now(),
                pad_connected: true,
                last_title: Instant::now(),
            })
        }
    }

    fn poll_gamepad(&mut self, input: &mut Input) {
        let Some(get_state) = self.xinput else { return };
        // Nicht verbundene Pads sind teuer abzufragen → nur alle 2 s erneut.
        if !self.pad_connected && self.pad_retry.elapsed() < Duration::from_secs(2) {
            return;
        }
        let mut st = XINPUT_STATE::default();
        // SAFETY: gültiger Zeiger auf eine XINPUT_STATE-Struktur.
        let rc = unsafe { get_state(0, &mut st) };
        if rc != ERROR_SUCCESS {
            if self.pad_connected {
                input.set_pad(Gamepad::default());
            }
            self.pad_connected = false;
            self.pad_retry = Instant::now();
            return;
        }
        self.pad_connected = true;
        let g = st.Gamepad;
        let stick = |x: i16, y: i16, dead: f32| {
            let (fx, fy) = (x as f32 / 32767.0, y as f32 / 32767.0);
            let m = (fx * fx + fy * fy).sqrt();
            if m < dead {
                (0.0, 0.0)
            } else {
                let k = ((m - dead) / (1.0 - dead)).min(1.0) / m;
                (fx * k, fy * k)
            }
        };
        let mut left = stick(g.sThumbLX, g.sThumbLY, 0.24);
        // Steuerkreuz als Alternative zum Stick.
        let b = g.wButtons;
        if b & 0x000F != 0 {
            left.0 = ((b & 0x0008 != 0) as i32 - (b & 0x0004 != 0) as i32) as f32;
            left.1 = ((b & 0x0001 != 0) as i32 - (b & 0x0002 != 0) as i32) as f32;
        }
        let mut buttons = 0;
        for (xb, pb) in [
            (0x1000, PAD_A),
            (0x2000, PAD_B),
            (0x4000, PAD_X),
            (0x8000, PAD_Y),
            (0x0010, PAD_START),
            (0x0020, PAD_BACK),
            (0x0100, PAD_LB),
            (0x0200, PAD_RB),
        ] {
            if b & xb != 0 {
                buttons |= pb;
            }
        }
        input.set_pad(Gamepad {
            connected: true,
            left,
            right: stick(g.sThumbRX, g.sThumbRY, 0.27),
            buttons,
            ..Default::default()
        });
    }
}

fn load_dwm_flush() -> Option<DwmFlushFn> {
    let name = wide("dwmapi.dll");
    // SAFETY: LoadLibrary/GetProcAddress mit nullterminierten Namen; die
    // Signatur entspricht der dokumentierten `HRESULT DwmFlush(void)`.
    unsafe {
        let m = LoadLibraryW(name.as_ptr());
        if m.is_null() {
            return None;
        }
        let p = GetProcAddress(m, c"DwmFlush".as_ptr().cast());
        if p.is_null() { None } else { Some(core::mem::transmute::<*mut c_void, DwmFlushFn>(p)) }
    }
}

fn load_xinput() -> Option<XInputGetStateFn> {
    for dll in ["xinput1_4.dll", "xinput1_3.dll", "xinput9_1_0.dll"] {
        let name = wide(dll);
        // SAFETY: wie oben; Signatur `DWORD XInputGetState(DWORD, XINPUT_STATE*)`.
        unsafe {
            let m = LoadLibraryW(name.as_ptr());
            if m.is_null() {
                continue;
            }
            let p = GetProcAddress(m, c"XInputGetState".as_ptr().cast());
            if !p.is_null() {
                return Some(core::mem::transmute::<*mut c_void, XInputGetStateFn>(p));
            }
        }
    }
    None
}

impl Platform for Win32Window {
    fn pump(&mut self, input: &mut Input) -> bool {
        let mut msg =
            MSG { hwnd: null_mut(), message: 0, wParam: 0, lParam: 0, time: 0, pt: POINT::default(), lPrivate: 0 };
        // SAFETY: `msg` ist gültig; Nachrichten werden normal verteilt.
        unsafe {
            while PeekMessageW(&mut msg, null_mut(), 0, 0, PM_REMOVE) != 0 {
                match msg.message {
                    WM_QUIT => CLOSE_REQUESTED.store(true, Ordering::Relaxed),
                    WM_KEYDOWN | WM_KEYUP | WM_SYSKEYDOWN | WM_SYSKEYUP => {
                        let down = msg.message == WM_KEYDOWN || msg.message == WM_SYSKEYDOWN;
                        let alt = (msg.lParam >> 29) & 1 != 0;
                        if down && alt && msg.wParam == 0x73 {
                            CLOSE_REQUESTED.store(true, Ordering::Relaxed); // Alt+F4
                        } else if down && alt && msg.wParam == 0x0D {
                            self.toggle_fullscreen(); // Alt+Enter
                        } else if let Some(k) = map_vk(msg.wParam) {
                            input.set_key(k, down);
                        }
                        // Nicht weiterreichen: verhindert das Systemmenü (Alt/F10).
                    }
                    _ => {
                        DispatchMessageW(&msg);
                    }
                }
            }
        }
        if FOCUS_LOST.swap(false, Ordering::Relaxed) {
            input.release_all();
        }
        self.poll_gamepad(input);
        !CLOSE_REQUESTED.load(Ordering::Relaxed)
    }

    fn present(&mut self, fb: &Framebuffer) {
        let mut rc = RECT::default();
        // SAFETY: Fenster und DC leben so lange wie `self`; Pixelpuffer ist
        // `width*height` u32 groß und passt zur BITMAPINFO.
        unsafe {
            GetClientRect(self.hwnd, &mut rc);
            let (cw, ch) = (rc.right - rc.left, rc.bottom - rc.top);
            if cw <= 0 || ch <= 0 {
                // Minimiert: nichts zu zeigen – CPU schonen statt leer zu drehen.
                std::thread::sleep(Duration::from_millis(15));
                return;
            }
            let (fw, fh) = (fb.width() as i32, fb.height() as i32);
            let fit = (cw as f32 / fw as f32).min(ch as f32 / fh as f32);
            // Ganzzahlig skalieren, wenn das kaum Fläche kostet (gestochen scharf).
            let s = if fit >= 2.0 && fit.floor() / fit >= 0.85 { fit.floor() } else { fit };
            let (dw, dh) = ((fw as f32 * s) as i32, (fh as f32 * s) as i32);
            let (dx, dy) = ((cw - dw) / 2, (ch - dh) / 2);
            if dx > 0 {
                PatBlt(self.hdc, 0, 0, dx, ch, BLACKNESS);
                PatBlt(self.hdc, dx + dw, 0, cw - dx - dw, ch, BLACKNESS);
            }
            if dy > 0 {
                PatBlt(self.hdc, 0, 0, cw, dy, BLACKNESS);
                PatBlt(self.hdc, 0, dy + dh, cw, ch - dy - dh, BLACKNESS);
            }
            self.bmi.bmiHeader.biWidth = fw;
            self.bmi.bmiHeader.biHeight = -fh;
            StretchDIBits(
                self.hdc,
                dx,
                dy,
                dw,
                dh,
                0,
                0,
                fw,
                fh,
                fb.color.as_ptr() as *const c_void,
                &self.bmi,
                DIB_RGB_COLORS,
                SRCCOPY,
            );
        }
    }

    fn set_title(&mut self, title: &str) {
        // Schutz vor zu häufigen Titeländerungen (der Runner ruft ohnehin nur
        // bei geänderter Statistik auf, also ca. 4× pro Sekunde).
        if self.last_title.elapsed() < Duration::from_millis(50) {
            return;
        }
        self.last_title = Instant::now();
        let t = wide(title);
        // SAFETY: nullterminierter UTF-16-String.
        unsafe { SetWindowTextW(self.hwnd, t.as_ptr()) };
    }

    fn wait_vsync(&mut self) -> bool {
        match self.dwm_flush {
            // SAFETY: Funktionszeiger aus dwmapi.dll mit passender Signatur.
            Some(f) => unsafe { f() >= 0 },
            None => false,
        }
    }

    fn toggle_fullscreen(&mut self) {
        // SAFETY: Raymond Chens Muster für randloses Vollbild; alle Strukturen
        // sind korrekt initialisiert (cbSize/length gesetzt).
        unsafe {
            let style = GetWindowLongPtrW(self.hwnd, GWL_STYLE) as u32;
            if !self.fullscreen {
                let mut mi = MONITORINFO { cbSize: core::mem::size_of::<MONITORINFO>() as u32, ..Default::default() };
                if GetWindowPlacement(self.hwnd, &mut self.placement) != 0
                    && GetMonitorInfoW(MonitorFromWindow(self.hwnd, MONITOR_DEFAULTTOPRIMARY), &mut mi) != 0
                {
                    SetWindowLongPtrW(self.hwnd, GWL_STYLE, (style & !WS_OVERLAPPEDWINDOW) as isize);
                    let m = mi.rcMonitor;
                    SetWindowPos(
                        self.hwnd,
                        null_mut(),
                        m.left,
                        m.top,
                        m.right - m.left,
                        m.bottom - m.top,
                        SWP_NOOWNERZORDER | SWP_FRAMECHANGED,
                    );
                    self.fullscreen = true;
                }
            } else {
                SetWindowLongPtrW(self.hwnd, GWL_STYLE, (style | self.style) as isize);
                SetWindowPlacement(self.hwnd, &self.placement);
                SetWindowPos(
                    self.hwnd,
                    null_mut(),
                    0,
                    0,
                    0,
                    0,
                    SWP_NOMOVE | SWP_NOSIZE | SWP_NOZORDER | SWP_NOOWNERZORDER | SWP_FRAMECHANGED,
                );
                self.fullscreen = false;
            }
        }
    }
}

impl Drop for Win32Window {
    fn drop(&mut self) {
        // SAFETY: Handles stammen aus `new` und werden genau einmal freigegeben.
        unsafe {
            ReleaseDC(self.hwnd, self.hdc);
            DestroyWindow(self.hwnd);
            timeEndPeriod(1);
        }
    }
}

/// Unsichtbares Blit-Ziel (Speicher-DC mit DIB-Section). Misst bzw. testet
/// den GDI-Present-Pfad, ohne ein Fenster zu öffnen.
pub struct OffscreenBlit {
    hdc: HDC,
    bitmap: *mut c_void,
    old: *mut c_void,
    bits: *mut u32,
    w: i32,
    h: i32,
    bmi: BITMAPINFO,
}

impl OffscreenBlit {
    /// Legt ein unsichtbares Ziel der Größe `w`×`h` an.
    pub fn new(w: usize, h: usize) -> Option<Self> {
        let bmi_dst = bitmap_info(w as i32, h as i32);
        // SAFETY: Speicher-DC ohne Fenster; Rückgaben werden geprüft.
        unsafe {
            let hdc = CreateCompatibleDC(null_mut());
            if hdc.is_null() {
                return None;
            }
            let mut bits: *mut c_void = null_mut();
            let bitmap = CreateDIBSection(hdc, &bmi_dst, DIB_RGB_COLORS, &mut bits, null_mut(), 0);
            if bitmap.is_null() || bits.is_null() {
                DeleteDC(hdc);
                return None;
            }
            let old = SelectObject(hdc, bitmap);
            SetStretchBltMode(hdc, COLORONCOLOR);
            Some(Self { hdc, bitmap, old, bits: bits as *mut u32, w: w as i32, h: h as i32, bmi: bitmap_info(1, 1) })
        }
    }

    /// Skaliert den Framebuffer wie im Fenster auf die volle Zielgröße.
    pub fn blit(&mut self, fb: &Framebuffer) {
        self.bmi.bmiHeader.biWidth = fb.width() as i32;
        self.bmi.bmiHeader.biHeight = -(fb.height() as i32);
        // SAFETY: DC/Bitmap leben bis Drop; Quellpuffer passt zur BITMAPINFO.
        unsafe {
            StretchDIBits(
                self.hdc,
                0,
                0,
                self.w,
                self.h,
                0,
                0,
                fb.width() as i32,
                fb.height() as i32,
                fb.color.as_ptr() as *const c_void,
                &self.bmi,
                DIB_RGB_COLORS,
                SRCCOPY,
            );
            GdiFlush();
        }
    }

    /// Pixel des Ziels (oben links beginnend).
    pub fn pixels(&self) -> &[u32] {
        // SAFETY: DIB-Section mit w*h 32-Bit-Pixeln, lebt bis Drop.
        unsafe { core::slice::from_raw_parts(self.bits, (self.w * self.h) as usize) }
    }
}

impl Drop for OffscreenBlit {
    fn drop(&mut self) {
        // SAFETY: Handles aus `new`, genau einmal freigegeben.
        unsafe {
            SelectObject(self.hdc, self.old);
            DeleteObject(self.bitmap);
            DeleteDC(self.hdc);
        }
    }
}

/// Erzeugt ein reines Nachrichtenfenster (`HWND_MESSAGE`, nie sichtbar),
/// verarbeitet seine Nachrichten und zerstört es wieder. Prüft headless,
/// dass Klassenregistrierung, Fenstererzeugung und Nachrichtenschleife
/// funktionieren.
pub fn message_window_smoke_test() -> bool {
    let class = wide("SpatzEngineWindow");
    let title = wide("spatz-test");
    // SAFETY: HWND_MESSAGE-Fenster kann weder angezeigt werden noch Fokus erhalten.
    unsafe {
        let hinst = register_class(&class);
        let hwnd_message = -3isize as HWND;
        let hwnd = CreateWindowExW(
            0,
            class.as_ptr(),
            title.as_ptr(),
            0,
            0,
            0,
            0,
            0,
            hwnd_message,
            null_mut(),
            hinst,
            null_mut(),
        );
        if hwnd.is_null() {
            return false;
        }
        let mut msg =
            MSG { hwnd: null_mut(), message: 0, wParam: 0, lParam: 0, time: 0, pt: POINT::default(), lPrivate: 0 };
        while PeekMessageW(&mut msg, hwnd, 0, 0, PM_REMOVE) != 0 {
            DispatchMessageW(&msg);
        }
        DestroyWindow(hwnd) != 0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn struct_layouts_match_win64() {
        assert_eq!(core::mem::size_of::<WNDCLASSEXW>(), 80);
        assert_eq!(core::mem::size_of::<MSG>(), 48);
        assert_eq!(core::mem::size_of::<BITMAPINFOHEADER>(), 40);
        assert_eq!(core::mem::size_of::<BITMAPINFO>(), 44);
        assert_eq!(core::mem::size_of::<MONITORINFO>(), 40);
        assert_eq!(core::mem::size_of::<WINDOWPLACEMENT>(), 44);
        assert_eq!(core::mem::size_of::<XINPUT_STATE>(), 16);
    }

    #[test]
    fn key_mapping() {
        assert_eq!(map_vk(0x41), Some(Key::A));
        assert_eq!(map_vk(0x5A), Some(Key::Z));
        assert_eq!(map_vk(0x30), Some(Key::Num0));
        assert_eq!(map_vk(0x70), Some(Key::F1));
        assert_eq!(map_vk(0x7B), Some(Key::F12));
        assert_eq!(map_vk(0x20), Some(Key::Space));
    }

    #[test]
    fn message_only_window_works() {
        assert!(message_window_smoke_test());
    }

    #[test]
    fn offscreen_blit_scales_correctly() {
        let mut fb = Framebuffer::new(4, 2);
        fb.color.copy_from_slice(&[0xFF0000, 0x00FF00, 0x0000FF, 0xFFFFFF, 0x111111, 0x222222, 0x333333, 0x444444]);
        let mut blit = OffscreenBlit::new(8, 4).expect("Speicher-DC");
        blit.blit(&fb);
        let px = blit.pixels();
        // Oben links rot (oben→unten-DIB korrekt), 2× Nearest-Skalierung.
        assert_eq!(px[0] & 0xFFFFFF, 0xFF0000);
        assert_eq!(px[1] & 0xFFFFFF, 0xFF0000);
        assert_eq!(px[2] & 0xFFFFFF, 0x00FF00);
        assert_eq!(px[8 * 3 + 7] & 0xFFFFFF, 0x444444);
    }
}
