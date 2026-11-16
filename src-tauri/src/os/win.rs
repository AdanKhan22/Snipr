//! Windows backend: low-level keyboard/mouse hooks + SendInput.

use super::KeyEvent;
use std::ffi::c_void;
use std::mem::size_of;
use std::sync::mpsc::Sender;
use std::sync::OnceLock;
use std::thread;
use std::time::{Duration, Instant};

use windows::core::{PCWSTR, PWSTR};
use windows::Win32::Media::Audio::{PlaySoundW, SND_ASYNC, SND_MEMORY, SND_NODEFAULT};
use windows::Win32::Foundation::{CloseHandle, HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::System::Threading::{
    OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION,
};
use windows::Win32::UI::Input::KeyboardAndMouse::*;
use windows::Win32::UI::WindowsAndMessaging::*;

pub const SUPPORTED: bool = true;

static SENDER: OnceLock<Sender<KeyEvent>> = OnceLock::new();

// ------------------------------------------------------------------ listening

pub fn start_listener(tx: Sender<KeyEvent>) {
    let _ = SENDER.set(tx);
    thread::Builder::new()
        .name("snipr-hooks".into())
        .spawn(|| unsafe {
            let hmod = GetModuleHandleW(None).ok().map(|m| windows::Win32::Foundation::HINSTANCE(m.0));
            let kb = SetWindowsHookExW(WH_KEYBOARD_LL, Some(keyboard_proc), hmod, 0);
            let ms = SetWindowsHookExW(WH_MOUSE_LL, Some(mouse_proc), hmod, 0);
            if kb.is_err() {
                eprintln!("snipr: failed to install keyboard hook");
                return;
            }
            let _ = ms;
            let mut msg = MSG::default();
            while GetMessageW(&mut msg, None, 0, 0).0 > 0 {
                let _ = TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }
        })
        .expect("spawn hook thread");
}

unsafe extern "system" fn mouse_proc(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if code == HC_ACTION as i32 {
        let m = wparam.0 as u32;
        if matches!(m, WM_LBUTTONDOWN | WM_RBUTTONDOWN | WM_MBUTTONDOWN) {
            if let Some(tx) = SENDER.get() {
                let _ = tx.send(KeyEvent::Reset);
            }
        }
    }
    CallNextHookEx(None, code, wparam, lparam)
}

unsafe extern "system" fn keyboard_proc(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if code == HC_ACTION as i32 {
        let kb = &*(lparam.0 as *const KBDLLHOOKSTRUCT);
        let msg = wparam.0 as u32;
        let injected = kb.flags.0 & LLKHF_INJECTED.0 != 0;
        if !injected && (msg == WM_KEYDOWN || msg == WM_SYSKEYDOWN) {
            if let Some(tx) = SENDER.get() {
                for ev in translate(kb) {
                    let _ = tx.send(ev);
                }
            }
        }
    }
    CallNextHookEx(None, code, wparam, lparam)
}

fn down(vk: VIRTUAL_KEY) -> bool {
    unsafe { (GetAsyncKeyState(vk.0 as i32) as u16) & 0x8000 != 0 }
}

unsafe fn translate(kb: &KBDLLHOOKSTRUCT) -> Vec<KeyEvent> {
    let vk = VIRTUAL_KEY(kb.vkCode as u16);
    match vk {
        VK_BACK => return vec![KeyEvent::Backspace],
        VK_SHIFT | VK_LSHIFT | VK_RSHIFT | VK_CONTROL | VK_LCONTROL | VK_RCONTROL | VK_MENU | VK_LMENU
        | VK_RMENU | VK_CAPITAL | VK_NUMLOCK | VK_SCROLL | VK_LWIN | VK_RWIN => return vec![],
        _ => {}
    }
    let ctrl = down(VK_CONTROL);
    let alt = down(VK_MENU);
    if down(VK_LWIN) || down(VK_RWIN) || (ctrl != alt) {
        // Shortcut (Ctrl+X, Alt+X, Win+X). Ctrl+Alt together is AltGr, which types characters.
        return vec![KeyEvent::Reset];
    }

    let mut state = [0u8; 256];
    for k in [VK_SHIFT, VK_LSHIFT, VK_RSHIFT, VK_CONTROL, VK_LCONTROL, VK_RCONTROL, VK_MENU, VK_LMENU, VK_RMENU] {
        if down(k) {
            state[k.0 as usize] = 0x80;
        }
    }
    if GetKeyState(VK_CAPITAL.0 as i32) & 1 != 0 {
        state[VK_CAPITAL.0 as usize] = 1;
    }
    let fg = GetForegroundWindow();
    let tid = GetWindowThreadProcessId(fg, None);
    let layout = GetKeyboardLayout(tid);
    let mut buf = [0u16; 8];
    // Flag 0x4: do not modify the keyboard state (keeps dead keys working for the real app).
    let n = ToUnicodeEx(kb.vkCode, kb.scanCode, &state, &mut buf, 0x4, Some(layout));
    if n < 0 {
        return vec![]; // dead key: wait for the composed character
    }
    if n == 0 {
        return vec![KeyEvent::Reset]; // non-printing key (arrows, Enter, F-keys...)
    }
    let s = String::from_utf16_lossy(&buf[..n as usize]);
    let mut out = vec![];
    for c in s.chars() {
        if c.is_control() {
            return vec![KeyEvent::Reset];
        }
        out.push(KeyEvent::Char(c));
    }
    out
}

// ------------------------------------------------------------ window & process

pub fn foreground_handle() -> isize {
    unsafe { GetForegroundWindow().0 as isize }
}

pub fn focus_window(h: isize) {
    if h == 0 {
        return;
    }
    unsafe {
        let hwnd = HWND(h as *mut c_void);
        if IsIconic(hwnd).as_bool() {
            let _ = ShowWindow(hwnd, SW_RESTORE);
        }
        let _ = SetForegroundWindow(hwnd);
    }
}

/// Lower-cased executable name (e.g. `chrome.exe`) of the window's process.
pub fn app_name_for(h: isize) -> Option<String> {
    if h == 0 {
        return None;
    }
    unsafe {
        let mut pid = 0u32;
        GetWindowThreadProcessId(HWND(h as *mut c_void), Some(&mut pid));
        if pid == 0 {
            return None;
        }
        let handle = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid).ok()?;
        let mut buf = [0u16; 520];
        let mut len = buf.len() as u32;
        let r = QueryFullProcessImageNameW(handle, PROCESS_NAME_WIN32, PWSTR(buf.as_mut_ptr()), &mut len);
        let _ = CloseHandle(handle);
        r.ok()?;
        let path = String::from_utf16_lossy(&buf[..len as usize]);
        path.rsplit(['\\', '/']).next().map(|s| s.to_lowercase())
    }
}

/// Waits (up to ~400ms) until the user releases Shift/Ctrl/Alt/Win so injected keys are clean.
pub fn wait_modifiers_released() {
    let start = Instant::now();
    while start.elapsed() < Duration::from_millis(400) {
        let held = [VK_SHIFT, VK_CONTROL, VK_MENU, VK_LWIN, VK_RWIN].iter().any(|k| down(*k));
        if !held {
            return;
        }
        thread::sleep(Duration::from_millis(10));
    }
}

// ------------------------------------------------------------------ injection

fn send(inputs: &[INPUT]) {
    for chunk in inputs.chunks(48) {
        unsafe {
            SendInput(chunk, size_of::<INPUT>() as i32);
        }
        if inputs.len() > 48 {
            thread::sleep(Duration::from_millis(2));
        }
    }
}

fn vk_input(vk: VIRTUAL_KEY, up: bool) -> INPUT {
    let extended = matches!(
        vk,
        VK_LEFT | VK_RIGHT | VK_UP | VK_DOWN | VK_HOME | VK_END | VK_PRIOR | VK_NEXT | VK_INSERT | VK_DELETE
    );
    let mut flags = KEYBD_EVENT_FLAGS(0);
    if up {
        flags |= KEYEVENTF_KEYUP;
    }
    if extended {
        flags |= KEYEVENTF_EXTENDEDKEY;
    }
    INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 {
            ki: KEYBDINPUT { wVk: vk, wScan: 0, dwFlags: flags, time: 0, dwExtraInfo: 0 },
        },
    }
}

fn unicode_input(unit: u16, up: bool) -> INPUT {
    let mut flags = KEYEVENTF_UNICODE;
    if up {
        flags |= KEYEVENTF_KEYUP;
    }
    INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 { ki: KEYBDINPUT { wVk: VIRTUAL_KEY(0), wScan: unit, dwFlags: flags, time: 0, dwExtraInfo: 0 } },
    }
}

fn tap(vk: VIRTUAL_KEY, times: usize) {
    let mut v = Vec::with_capacity(times * 2);
    for _ in 0..times {
        v.push(vk_input(vk, false));
        v.push(vk_input(vk, true));
    }
    send(&v);
}

pub fn send_backspaces(n: usize) {
    tap(VK_BACK, n);
}

pub fn send_left(n: usize) {
    tap(VK_LEFT, n);
}

static WAV: OnceLock<Vec<u8>> = OnceLock::new();

/// A short, soft "pop" synthesised in memory (no asset files needed).
fn pop_wav() -> &'static Vec<u8> {
    WAV.get_or_init(|| {
        let rate = 44_100u32;
        let samples = (rate as f32 * 0.09) as usize;
        let mut pcm: Vec<u8> = Vec::with_capacity(samples * 2);
        for i in 0..samples {
            let t = i as f32 / rate as f32;
            let freq = 520.0 + 700.0 * (1.0 - (t / 0.09)); // quick downward chirp
            let env = (-t * 48.0).exp() * (1.0 - (-t * 900.0).exp());
            let v = (2.0 * std::f32::consts::PI * freq * t).sin() * env * 0.35;
            pcm.extend_from_slice(&((v * i16::MAX as f32) as i16).to_le_bytes());
        }
        let mut w = Vec::with_capacity(44 + pcm.len());
        w.extend_from_slice(b"RIFF");
        w.extend_from_slice(&(36 + pcm.len() as u32).to_le_bytes());
        w.extend_from_slice(b"WAVEfmt ");
        w.extend_from_slice(&16u32.to_le_bytes());
        w.extend_from_slice(&1u16.to_le_bytes()); // PCM
        w.extend_from_slice(&1u16.to_le_bytes()); // mono
        w.extend_from_slice(&rate.to_le_bytes());
        w.extend_from_slice(&(rate * 2).to_le_bytes());
        w.extend_from_slice(&2u16.to_le_bytes());
        w.extend_from_slice(&16u16.to_le_bytes());
        w.extend_from_slice(b"data");
        w.extend_from_slice(&(pcm.len() as u32).to_le_bytes());
        w.extend_from_slice(&pcm);
        w
    })
}

pub fn play_sound() {
    let wav = pop_wav();
    unsafe {
        let _ = PlaySoundW(PCWSTR(wav.as_ptr() as *const u16), None, SND_MEMORY | SND_ASYNC | SND_NODEFAULT);
    }
}

pub fn send_paste() {
    send(&[
        vk_input(VK_CONTROL, false),
        vk_input(VIRTUAL_KEY(b'V' as u16), false),
        vk_input(VIRTUAL_KEY(b'V' as u16), true),
        vk_input(VK_CONTROL, true),
    ]);
}

/// Types text as Unicode keystrokes (no clipboard). Newlines become Enter.
pub fn send_text(s: &str) {
    type_text(s, false);
}

/// Like `send_text` but newlines are Shift+Enter, so chat apps insert a line break
/// instead of sending the message.
pub fn send_text_soft(s: &str) {
    type_text(s, true);
}

fn type_text(s: &str, soft_newline: bool) {
    let mut v = vec![];
    for c in s.chars() {
        match c {
            '\r' => {}
            '\n' => {
                if soft_newline {
                    v.push(vk_input(VK_SHIFT, false));
                }
                v.push(vk_input(VK_RETURN, false));
                v.push(vk_input(VK_RETURN, true));
                if soft_newline {
                    v.push(vk_input(VK_SHIFT, true));
                }
            }
            _ => {
                let mut b = [0u16; 2];
                for u in c.encode_utf16(&mut b) {
                    v.push(unicode_input(*u, false));
                    v.push(unicode_input(*u, true));
                }
            }
        }
    }
    send(&v);
}

fn key_by_name(name: &str) -> Option<VIRTUAL_KEY> {
    let n = name.trim().to_lowercase();
    Some(match n.as_str() {
        "enter" | "return" => VK_RETURN,
        "tab" => VK_TAB,
        "esc" | "escape" => VK_ESCAPE,
        "space" => VK_SPACE,
        "backspace" => VK_BACK,
        "delete" | "del" => VK_DELETE,
        "insert" => VK_INSERT,
        "home" => VK_HOME,
        "end" => VK_END,
        "pageup" => VK_PRIOR,
        "pagedown" => VK_NEXT,
        "left" => VK_LEFT,
        "right" => VK_RIGHT,
        "up" => VK_UP,
        "down" => VK_DOWN,
        "ctrl" | "control" => VK_CONTROL,
        "shift" => VK_SHIFT,
        "alt" => VK_MENU,
        "win" | "super" => VK_LWIN,
        _ => {
            if let Some(num) = n.strip_prefix('f').and_then(|x| x.parse::<u16>().ok()) {
                if (1..=12).contains(&num) {
                    return Some(VIRTUAL_KEY(VK_F1.0 + num - 1));
                }
            }
            let mut it = n.chars();
            match (it.next(), it.next()) {
                (Some(c), None) if c.is_ascii_alphanumeric() => VIRTUAL_KEY(c.to_ascii_uppercase() as u16),
                _ => return None,
            }
        }
    })
}

/// Sends a key or chord such as `enter`, `ctrl+shift+left`, `tab*3` (repeat).
pub fn send_combo(spec: &str) {
    let (combo, times) = match spec.split_once('*') {
        Some((c, t)) => (c, t.trim().parse::<usize>().unwrap_or(1).clamp(1, 50)),
        None => (spec, 1),
    };
    let keys: Vec<VIRTUAL_KEY> = combo.split('+').filter_map(key_by_name).collect();
    let Some((last, mods)) = keys.split_last() else { return };
    for _ in 0..times {
        let mut v = vec![];
        for m in mods {
            v.push(vk_input(*m, false));
        }
        v.push(vk_input(*last, false));
        v.push(vk_input(*last, true));
        for m in mods.iter().rev() {
            v.push(vk_input(*m, true));
        }
        send(&v);
    }
}
