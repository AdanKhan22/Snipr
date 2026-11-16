use crate::engine::Engine;
use crate::model::*;
use crate::{import, os, store};
use fuzzy_matcher::skim::SkimMatcherV2;
use fuzzy_matcher::FuzzyMatcher;
use parking_lot::Mutex;
use serde::Serialize;
use std::path::PathBuf;
use std::sync::atomic::{AtomicIsize, Ordering};
use std::sync::Arc;
use std::time::{Duration, SystemTime};
use tauri::{AppHandle, Emitter, Manager, State};

pub struct AppState {
    pub dir: PathBuf,
    pub library: Mutex<Library>,
    pub settings: Mutex<Settings>,
    pub engine: Arc<Engine>,
    pub prev_focus: AtomicIsize,
    pub last_mtime: Mutex<Option<SystemTime>>,
}

pub type St<'a> = State<'a, Arc<AppState>>;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StateDto {
    library: Library,
    settings: Settings,
    enabled: bool,
    supported: bool,
    version: String,
    library_file: String,
}

fn dto(st: &AppState) -> StateDto {
    let settings = st.settings.lock().clone();
    StateDto {
        library: st.library.lock().clone(),
        library_file: store::library_path(&st.dir, &settings).to_string_lossy().to_string(),
        settings,
        enabled: st.engine.is_enabled(),
        supported: os::SUPPORTED,
        version: env!("CARGO_PKG_VERSION").to_string(),
    }
}

/// Write the library to disk. `reload` also recompiles the engine's trigger table.
pub fn save(st: &AppState, reload: bool) {
    let settings = st.settings.lock().clone();
    let mut lib = st.library.lock();
    lib.counters = st.engine.counters();
    let path = store::library_path(&st.dir, &settings);
    if let Err(e) = store::save_library(&path, &lib, &store::backups_dir(&st.dir)) {
        eprintln!("snipr: could not save library: {e}");
    }
    *st.last_mtime.lock() = store::mtime(&path);
    if reload {
        st.engine.reload(&lib, &settings);
    }
}

fn changed(app: &AppHandle) {
    let _ = app.emit("library-changed", ());
}

pub fn record_use(app: &AppHandle, id: &str) {
    let st = app.state::<Arc<AppState>>();
    {
        let mut lib = st.library.lock();
        if let Some(s) = lib.snippets.iter_mut().find(|s| s.id == id) {
            s.use_count += 1;
            s.last_used = Some(now());
        }
    }
    save(&st, false);
    changed(app);
}

#[tauri::command]
pub fn get_state(st: St) -> StateDto {
    dto(&st)
}

#[tauri::command]
pub fn save_snippet(app: AppHandle, st: St, mut snippet: Snippet) -> Result<(), String> {
    if snippet.kind == MatchKind::Regex {
        regex::Regex::new(&snippet.trigger).map_err(|e| format!("Invalid regex: {e}"))?;
    }
    snippet.modified = now();
    {
        let mut lib = st.library.lock();
        if snippet.group_id.is_empty() || !lib.groups.iter().any(|g| g.id == snippet.group_id) {
            snippet.group_id = lib.groups.first().map(|g| g.id.clone()).unwrap_or_default();
        }
        if let Some(existing) = lib.snippets.iter_mut().find(|s| s.id == snippet.id) {
            snippet.use_count = existing.use_count;
            snippet.last_used = existing.last_used.clone();
            snippet.created = existing.created.clone();
            *existing = snippet;
        } else {
            lib.snippets.push(snippet);
        }
    }
    save(&st, true);
    changed(&app);
    Ok(())
}

#[tauri::command]
pub fn delete_snippets(app: AppHandle, st: St, ids: Vec<String>) {
    st.library.lock().snippets.retain(|s| !ids.contains(&s.id));
    save(&st, true);
    changed(&app);
}

#[tauri::command]
pub fn move_snippets(app: AppHandle, st: St, ids: Vec<String>, group_id: String) {
    for s in st.library.lock().snippets.iter_mut().filter(|s| ids.contains(&s.id)) {
        s.group_id = group_id.clone();
    }
    save(&st, true);
    changed(&app);
}

#[tauri::command]
pub fn save_group(app: AppHandle, st: St, group: Group) {
    {
        let mut lib = st.library.lock();
        if let Some(g) = lib.groups.iter_mut().find(|g| g.id == group.id) {
            *g = group;
        } else {
            lib.groups.push(group);
        }
    }
    save(&st, true);
    changed(&app);
}

#[tauri::command]
pub fn delete_group(app: AppHandle, st: St, id: String) {
    {
        let mut lib = st.library.lock();
        lib.groups.retain(|g| g.id != id);
        lib.snippets.retain(|s| s.group_id != id);
        if lib.groups.is_empty() {
            lib.groups.push(Group { name: "General".into(), ..Group::default() });
        }
    }
    save(&st, true);
    changed(&app);
}

#[tauri::command]
pub fn set_settings(app: AppHandle, st: St, settings: Settings) -> Result<(), String> {
    let old_path = store::library_path(&st.dir, &st.settings.lock());
    let new_path = store::library_path(&st.dir, &settings);
    *st.settings.lock() = settings.clone();
    store::save_settings(&st.dir, &settings).map_err(|e| e.to_string())?;

    if old_path != new_path {
        if new_path.exists() {
            // Switching to an existing synced library: load it.
            *st.library.lock() = store::load_library(&new_path);
            *st.last_mtime.lock() = store::mtime(&new_path);
            let lib = st.library.lock();
            st.engine.reload(&lib, &settings);
        } else {
            save(&st, true);
        }
    } else {
        let lib = st.library.lock();
        st.engine.reload(&lib, &settings);
    }
    crate::apply_system_settings(&app);
    changed(&app);
    Ok(())
}

#[tauri::command]
pub fn set_enabled(app: AppHandle, enabled: bool) {
    crate::set_enabled_everywhere(&app, enabled);
}

#[tauri::command]
pub fn preview_snippet(st: St, content: String) -> String {
    st.engine.preview(&content)
}

#[tauri::command]
pub fn search_snippets(st: St, query: String) -> Vec<Snippet> {
    let lib = st.library.lock();
    let q = query.trim();
    if q.is_empty() {
        let mut v: Vec<Snippet> = lib.snippets.iter().filter(|s| s.enabled).cloned().collect();
        v.sort_by(|a, b| b.last_used.cmp(&a.last_used).then(b.use_count.cmp(&a.use_count)));
        v.truncate(50);
        return v;
    }
    let m = SkimMatcherV2::default();
    let mut scored: Vec<(i64, &Snippet)> = lib
        .snippets
        .iter()
        .filter(|s| s.enabled)
        .filter_map(|s| {
            let name = m.fuzzy_match(&s.name, q).map(|x| x * 2);
            let trig = m.fuzzy_match(&s.trigger, q).map(|x| x * 3);
            let body = m.fuzzy_match(&s.content, q);
            let tags = s.tags.iter().filter_map(|t| m.fuzzy_match(t, q)).max();
            [name, trig, body, tags].into_iter().flatten().max().map(|sc| (sc, s))
        })
        .collect();
    scored.sort_by(|a, b| b.0.cmp(&a.0));
    scored.into_iter().take(50).map(|(_, s)| s.clone()).collect()
}

pub fn open_picker(app: &AppHandle) {
    let st = app.state::<Arc<AppState>>();
    st.prev_focus.store(os::foreground_handle(), Ordering::SeqCst);
    if let Some(w) = app.get_webview_window("picker") {
        let _ = app.emit_to("picker", "picker-open", ());
        let _ = w.center();
        let _ = w.show();
        let _ = w.set_focus();
    }
}

fn hide_picker(app: &AppHandle, st: &AppState) {
    if let Some(w) = app.get_webview_window("picker") {
        let _ = w.hide();
    }
    os::focus_window(st.prev_focus.load(Ordering::SeqCst));
}

#[tauri::command]
pub fn close_picker(app: AppHandle, st: St) {
    hide_picker(&app, &st);
}

#[tauri::command]
pub fn pick_snippet(app: AppHandle, st: St, id: String) {
    hide_picker(&app, &st);
    let engine = st.engine.clone();
    std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(150));
        engine.insert_snippet(&id);
    });
}

#[tauri::command]
pub fn submit_prompt(st: St, value: Option<String>) {
    st.engine.submit_prompt(value);
}

#[tauri::command]
pub fn import_file(app: AppHandle, st: St, path: String) -> Result<usize, String> {
    let text = std::fs::read_to_string(&path).map_err(|e| e.to_string())?;
    let mut lib = st.library.lock().clone();
    let n = if path.to_lowercase().ends_with(".csv") { import::import_csv(&text, &mut lib)? } else { import::import_json(&text, &mut lib)? };
    *st.library.lock() = lib;
    save(&st, true);
    changed(&app);
    Ok(n)
}

#[tauri::command]
pub fn export_file(st: St, path: String) -> Result<(), String> {
    let lib = st.library.lock().clone();
    std::fs::write(path, serde_json::to_string_pretty(&lib).unwrap()).map_err(|e| e.to_string())
}
