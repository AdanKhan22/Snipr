//! The expansion engine: watches typed characters, matches triggers, renders and injects snippets.

use crate::model::*;
use crate::os::{self, KeyEvent};
use crate::variables::{self, Ctx, Part};
use parking_lot::{Mutex, RwLock};
use regex::Regex;
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicIsize, Ordering};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::Arc;
use std::thread;
use std::time::Duration;
use tauri::{AppHandle, Emitter, Manager};

const MAX_BUFFER: usize = 128;

struct Compiled {
    s: Snippet,
    trig: Vec<char>,
    re: Option<Regex>,
}

struct Shared {
    app: AppHandle,
    enabled: AtomicBool,
    compiled: RwLock<Vec<Compiled>>,
    all: RwLock<Vec<Snippet>>,
    settings: RwLock<Settings>,
    counters: Mutex<HashMap<String, i64>>,
    prompt_tx: Mutex<Option<Sender<Option<String>>>>,
    own_exe: String,
    #[allow(dead_code)]
    last_focus: AtomicIsize,
}

pub struct Engine {
    shared: Arc<Shared>,
}

fn norm_app(s: &str) -> String {
    let s = s.trim().to_lowercase();
    s.strip_suffix(".exe").unwrap_or(&s).to_string()
}

fn lower(c: char) -> char {
    c.to_lowercase().next().unwrap_or(c)
}

impl Engine {
    pub fn new(app: AppHandle) -> Arc<Engine> {
        let own_exe = std::env::current_exe()
            .ok()
            .and_then(|p| p.file_name().map(|n| n.to_string_lossy().to_string()))
            .map(|n| norm_app(&n))
            .unwrap_or_default();
        Arc::new(Engine {
            shared: Arc::new(Shared {
                app,
                enabled: AtomicBool::new(true),
                compiled: RwLock::new(vec![]),
                all: RwLock::new(vec![]),
                settings: RwLock::new(Settings::default()),
                counters: Mutex::new(HashMap::new()),
                prompt_tx: Mutex::new(None),
                own_exe,
                last_focus: AtomicIsize::new(0),
            }),
        })
    }

    pub fn start(&self) {
        let (tx, rx) = channel();
        os::start_listener(tx);
        let sh = self.shared.clone();
        thread::Builder::new()
            .name("snipr-engine".into())
            .spawn(move || worker(sh, rx))
            .expect("spawn engine thread");
    }

    pub fn is_enabled(&self) -> bool {
        self.shared.enabled.load(Ordering::SeqCst)
    }

    pub fn set_enabled(&self, v: bool) {
        self.shared.enabled.store(v, Ordering::SeqCst);
    }

    pub fn counters(&self) -> HashMap<String, i64> {
        self.shared.counters.lock().clone()
    }

    pub fn reload(&self, lib: &Library, settings: &Settings) {
        *self.shared.settings.write() = settings.clone();
        *self.shared.counters.lock() = lib.counters.clone();
        *self.shared.all.write() = lib.snippets.clone();
        let mut compiled: Vec<Compiled> = vec![];
        for s in &lib.snippets {
            if !s.enabled || s.trigger.is_empty() {
                continue;
            }
            let group_ok = lib.groups.iter().find(|g| g.id == s.group_id).map(|g| g.enabled).unwrap_or(true);
            if !group_ok {
                continue;
            }
            match s.kind {
                MatchKind::Text => {
                    let trig: Vec<char> = if s.case_sensitive { s.trigger.chars().collect() } else { s.trigger.chars().map(lower).collect() };
                    compiled.push(Compiled { s: s.clone(), trig, re: None });
                }
                MatchKind::Regex => {
                    let pat = format!("{}(?:{})$", if s.case_sensitive { "" } else { "(?i)" }, s.trigger);
                    if let Ok(re) = Regex::new(&pat) {
                        compiled.push(Compiled { s: s.clone(), trig: vec![], re: Some(re) });
                    }
                }
            }
        }
        // Longest text triggers first so `;sigx` wins over `;sig`; regexes last.
        compiled.sort_by_key(|c| (c.re.is_some(), std::cmp::Reverse(c.trig.len())));
        *self.shared.compiled.write() = compiled;
    }

    /// Insert a snippet at the caret (used by the picker); no trigger is deleted.
    pub fn insert_snippet(&self, id: &str) {
        let sh = self.shared.clone();
        let snip = sh.all.read().iter().find(|s| s.id == id).cloned();
        if let Some(snip) = snip {
            thread::spawn(move || perform(&sh, &snip, 0, vec![], None, None));
        }
    }

    pub fn submit_prompt(&self, value: Option<String>) {
        if let Some(tx) = self.shared.prompt_tx.lock().take() {
            let _ = tx.send(value);
        }
    }

    /// Render content for the editor preview (no side effects).
    pub fn preview(&self, content: &str) -> String {
        let clipboard = arboard::Clipboard::new().and_then(|mut c| c.get_text()).unwrap_or_default();
        let sh = &self.shared;
        let lookup = |q: &str| find_content(sh, q);
        let prompt = |_: &str, _: &str| None;
        let counter = |_: &str| 0;
        let ctx = Ctx {
            clipboard,
            captures: vec!["match".into(), "group1".into(), "group2".into()],
            allow_shell: false,
            preview: true,
            lookup: &lookup,
            prompt: &prompt,
            counter: &counter,
        };
        match variables::render(content, &ctx) {
            Ok(parts) => parts
                .into_iter()
                .map(|p| match p {
                    Part::Text(t) => t,
                    Part::Cursor => "|".into(),
                    Part::Key(k) => format!("⟨{}⟩", k),
                })
                .collect(),
            Err(_) => String::new(),
        }
    }
}

fn find_content(sh: &Shared, q: &str) -> Option<String> {
    sh.all
        .read()
        .iter()
        .find(|s| s.name.eq_ignore_ascii_case(q) || s.trigger == q)
        .map(|s| s.content.clone())
}

impl Shared {
    /// Ask the user for a value through the small prompt window. Blocks the calling thread.
    fn prompt(&self, label: &str, default: &str) -> Option<String> {
        let (tx, rx) = channel();
        *self.prompt_tx.lock() = Some(tx);
        let prev = os::foreground_handle();
        let win = self.app.get_webview_window("prompt")?;
        let _ = self.app.emit_to("prompt", "prompt-open", serde_json::json!({ "label": label, "default": default }));
        let _ = win.center();
        let _ = win.show();
        let _ = win.set_focus();
        let r = rx.recv_timeout(Duration::from_secs(180)).ok().flatten();
        let _ = win.hide();
        os::focus_window(prev);
        thread::sleep(Duration::from_millis(150));
        r
    }
}

// --------------------------------------------------------------------- worker

struct Hit {
    snip: Snippet,
    typed: String,
    delete: usize,
    captures: Vec<String>,
}

fn app_allowed(s: &Snippet, app: &str) -> bool {
    match s.app_mode {
        AppRuleMode::All => true,
        AppRuleMode::Only => s.apps.iter().any(|a| norm_app(a) == app),
        AppRuleMode::Except => !s.apps.iter().any(|a| norm_app(a) == app),
    }
}

fn find(compiled: &[Compiled], buf: &[char], delim: bool, app: &str) -> Option<Hit> {
    let buf_str: String = buf.iter().collect();
    for c in compiled {
        let s = &c.s;
        if delim != (s.mode == TriggerMode::Delimiter) || !app_allowed(s, app) {
            continue;
        }
        if let Some(re) = &c.re {
            if let Some(caps) = re.captures(&buf_str) {
                let m = caps.get(0).unwrap();
                if m.as_str().is_empty() {
                    continue;
                }
                let captures = caps.iter().map(|g| g.map(|g| g.as_str().to_string()).unwrap_or_default()).collect();
                let n = m.as_str().chars().count();
                return Some(Hit { snip: s.clone(), typed: m.as_str().to_string(), delete: n + delim as usize, captures });
            }
            continue;
        }
        let n = c.trig.len();
        if n == 0 || buf.len() < n {
            continue;
        }
        let tail = &buf[buf.len() - n..];
        let eq = if s.case_sensitive { tail == c.trig.as_slice() } else { tail.iter().map(|c| lower(*c)).eq(c.trig.iter().copied()) };
        if !eq {
            continue;
        }
        if s.boundary == Boundary::Strict && c.trig[0].is_alphanumeric() && buf.len() > n && buf[buf.len() - n - 1].is_alphanumeric() {
            continue;
        }
        return Some(Hit { snip: s.clone(), typed: tail.iter().collect(), delete: n + delim as usize, captures: vec![] });
    }
    None
}

fn worker(sh: Arc<Shared>, rx: Receiver<KeyEvent>) {
    let mut buf: Vec<char> = vec![];
    let mut last_hwnd = 0isize;
    let mut app = String::new();
    while let Ok(ev) = rx.recv() {
        if !sh.enabled.load(Ordering::SeqCst) {
            buf.clear();
            continue;
        }
        let h = os::foreground_handle();
        if h != last_hwnd {
            last_hwnd = h;
            app = os::app_name_for(h).map(|a| norm_app(&a)).unwrap_or_default();
            buf.clear();
        }
        if app == sh.own_exe {
            continue; // never expand inside Snipr itself (editor, picker)
        }
        let blocked = sh.settings.read().disabled_apps.iter().any(|a| norm_app(a) == app);
        if blocked {
            buf.clear();
            continue;
        }
        match ev {
            KeyEvent::Reset => buf.clear(),
            KeyEvent::Backspace => {
                buf.pop();
            }
            KeyEvent::Char(c) => {
                let delim_char = sh.settings.read().delimiters.contains(c);
                let hit = {
                    let compiled = sh.compiled.read();
                    let mut hit = None;
                    if delim_char {
                        hit = find(&compiled, &buf, true, &app);
                    }
                    if hit.is_none() {
                        buf.push(c);
                        if buf.len() > MAX_BUFFER {
                            buf.remove(0);
                        }
                        hit = find(&compiled, &buf, false, &app);
                    }
                    hit
                };
                if let Some(hit) = hit {
                    let delim = if hit.delete > hit.typed.chars().count() { Some(c) } else { None };
                    buf.clear();
                    perform(&sh, &hit.snip, hit.delete, hit.captures.clone(), Some(&hit.typed), delim);
                    while rx.try_recv().is_ok() {}
                    buf.clear();
                }
            }
        }
    }
}

// ------------------------------------------------------------------ expansion

enum CaseMode {
    Upper,
    Capitalize,
}

fn case_mode(typed: &str, trigger: &str) -> Option<CaseMode> {
    if typed == trigger {
        return None;
    }
    let letters: Vec<char> = typed.chars().filter(|c| c.is_alphabetic()).collect();
    if letters.len() >= 2 && letters.iter().all(|c| c.is_uppercase()) {
        return Some(CaseMode::Upper);
    }
    let first_typed = letters.first()?;
    let first_trig = trigger.chars().find(|c| c.is_alphabetic())?;
    if first_typed.is_uppercase() && !first_trig.is_uppercase() {
        return Some(CaseMode::Capitalize);
    }
    None
}

fn perform(sh: &Arc<Shared>, snip: &Snippet, delete: usize, captures: Vec<String>, typed: Option<&str>, delim: Option<char>) {
    let settings = sh.settings.read().clone();
    let clipboard = arboard::Clipboard::new().and_then(|mut c| c.get_text()).unwrap_or_default();
    let lookup = |q: &str| find_content(sh, q);
    let prompt = |l: &str, d: &str| sh.prompt(l, d);
    let counter = |name: &str| {
        let mut c = sh.counters.lock();
        let e = c.entry(name.to_string()).or_insert(0);
        *e += 1;
        *e
    };
    let ctx = Ctx {
        clipboard,
        captures,
        allow_shell: settings.allow_shell,
        preview: false,
        lookup: &lookup,
        prompt: &prompt,
        counter: &counter,
    };
    let Ok(mut parts) = variables::render(&snip.content, &ctx) else { return };

    if let (Some(typed), true, MatchKind::Text) = (typed, snip.case_propagate && !snip.case_sensitive, snip.kind) {
        match case_mode(typed, &snip.trigger) {
            Some(CaseMode::Upper) => {
                for p in parts.iter_mut() {
                    if let Part::Text(t) = p {
                        *t = t.to_uppercase();
                    }
                }
            }
            Some(CaseMode::Capitalize) => {
                if let Some(Part::Text(t)) = parts.iter_mut().find(|p| matches!(p, Part::Text(_))) {
                    let mut ch = t.chars();
                    if let Some(f) = ch.next() {
                        *t = f.to_uppercase().chain(ch).collect();
                    }
                }
            }
            None => {}
        }
    }
    if let Some(d) = delim {
        parts.push(Part::Text(d.to_string()));
    }

    if settings.sound_on_expand {
        os::play_sound();
    }
    os::wait_modifiers_released();
    thread::sleep(Duration::from_millis(40));
    if delete > 0 {
        os::send_backspaces(delete);
        thread::sleep(Duration::from_millis(60));
    }
    inject(&parts, &settings);
    crate::commands::record_use(&sh.app, &snip.id);
}

enum Saved {
    Text(String),
    Image(arboard::ImageData<'static>),
    Nothing,
}

fn set_clipboard(text: &str) {
    let text = if cfg!(windows) { text.replace("\r\n", "\n").replace('\n', "\r\n") } else { text.to_string() };
    for _ in 0..5 {
        if arboard::Clipboard::new().and_then(|mut c| c.set_text(text.clone())).is_ok() {
            return;
        }
        thread::sleep(Duration::from_millis(20));
    }
}

fn inject(parts: &[Part], settings: &Settings) {
    let mut saved: Option<Saved> = None;

    // Text and cursor markers between two key presses are inserted as one block, so a
    // cursor in the middle of a snippet never splits it into several pastes.
    //
    // With a cursor, the text before it is pasted and the text after it is typed (when
    // `exact_cursor` is on): rich editors reshape pasted line breaks, which makes counting
    // Left-arrow presses unreliable, whereas typed characters move the caret predictably.
    let paste = |text: &str, saved: &mut Option<Saved>| {
        if text.is_empty() {
            return;
        }
        match settings.paste_method {
            PasteMethod::Type => os::send_text(text),
            PasteMethod::Paste => {
                if saved.is_none() {
                    *saved = Some(match arboard::Clipboard::new() {
                        Ok(mut c) => match c.get_text() {
                            Ok(txt) => Saved::Text(txt),
                            Err(_) => c.get_image().map(Saved::Image).unwrap_or(Saved::Nothing),
                        },
                        Err(_) => Saved::Nothing,
                    });
                }
                set_clipboard(text);
                thread::sleep(Duration::from_millis(30));
                os::send_paste();
                // Browsers read the clipboard asynchronously; give them time.
                thread::sleep(Duration::from_millis(150));
            }
        }
    };
    let flush = |text: &mut String, cursor_at: &mut Option<usize>, saved: &mut Option<Saved>| {
        match cursor_at.take() {
            Some(idx) => {
                let (before, after) = text.split_at(idx);
                paste(before, saved);
                if !after.is_empty() {
                    if settings.exact_cursor && settings.paste_method == PasteMethod::Paste {
                        os::send_text_soft(after);
                    } else {
                        paste(after, saved);
                    }
                    thread::sleep(Duration::from_millis(30));
                    os::send_left(after.chars().filter(|c| *c != '\r').count());
                }
            }
            None => paste(text, saved),
        }
        text.clear();
    };

    let mut text = String::new();
    let mut cursor_at: Option<usize> = None;
    for p in parts {
        match p {
            Part::Text(t) => text.push_str(t),
            Part::Cursor => cursor_at = Some(text.len()),
            Part::Key(k) => {
                flush(&mut text, &mut cursor_at, &mut saved);
                os::send_combo(k);
                thread::sleep(Duration::from_millis(25));
            }
        }
    }
    flush(&mut text, &mut cursor_at, &mut saved);

    if settings.restore_clipboard {
        thread::sleep(Duration::from_millis(400));
        match saved {
            Some(Saved::Text(t)) => {
                if let Ok(mut c) = arboard::Clipboard::new() {
                    let _ = c.set_text(t);
                }
            }
            Some(Saved::Image(img)) => {
                if let Ok(mut c) = arboard::Clipboard::new() {
                    let _ = c.set_image(img);
                }
            }
            _ => {}
        }
    }
}
