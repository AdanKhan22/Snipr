use crate::model::{Library, Settings};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

pub fn settings_path(dir: &Path) -> PathBuf {
    dir.join("settings.json")
}

pub fn backups_dir(dir: &Path) -> PathBuf {
    dir.join("backups")
}

pub fn load_settings(dir: &Path) -> Settings {
    fs::read_to_string(settings_path(dir))
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

pub fn save_settings(dir: &Path, s: &Settings) -> std::io::Result<()> {
    fs::create_dir_all(dir)?;
    write_atomic(&settings_path(dir), &serde_json::to_string_pretty(s).unwrap())
}

/// Resolve where the library file lives (custom sync folder or app data).
pub fn library_path(dir: &Path, s: &Settings) -> PathBuf {
    match s.library_path.as_deref().map(str::trim).filter(|p| !p.is_empty()) {
        Some(p) => {
            let p = PathBuf::from(p);
            if p.extension().is_some_and(|e| e == "json") {
                p
            } else {
                p.join("library.json")
            }
        }
        None => dir.join("library.json"),
    }
}

pub fn load_library(path: &Path) -> Library {
    match fs::read_to_string(path) {
        Ok(text) => serde_json::from_str(&text).unwrap_or_else(|_| {
            // Corrupt file: keep a copy so nothing is lost, start fresh.
            let _ = fs::copy(path, path.with_extension("corrupt.json"));
            Library::seeded()
        }),
        Err(_) => Library::seeded(),
    }
}

pub fn save_library(path: &Path, lib: &Library, backups: &Path) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    if path.exists() {
        let _ = backup(path, backups);
    }
    write_atomic(path, &serde_json::to_string_pretty(lib).unwrap())
}

pub fn mtime(path: &Path) -> Option<SystemTime> {
    fs::metadata(path).and_then(|m| m.modified()).ok()
}

fn write_atomic(path: &Path, data: &str) -> std::io::Result<()> {
    let tmp = path.with_extension("tmp");
    fs::write(&tmp, data)?;
    fs::rename(&tmp, path)
}

/// Rolling backups: at most one per 10 minutes, newest 20 kept.
fn backup(src: &Path, dir: &Path) -> std::io::Result<()> {
    fs::create_dir_all(dir)?;
    let mut files: Vec<(SystemTime, PathBuf)> = fs::read_dir(dir)?
        .filter_map(|e| e.ok())
        .filter_map(|e| Some((e.metadata().ok()?.modified().ok()?, e.path())))
        .collect();
    files.sort_by_key(|f| f.0);
    if let Some((t, _)) = files.last() {
        if t.elapsed().unwrap_or(Duration::MAX) < Duration::from_secs(600) {
            return Ok(());
        }
    }
    let name = format!("library-{}.json", chrono::Local::now().format("%Y%m%d-%H%M%S"));
    fs::copy(src, dir.join(name))?;
    while files.len() >= 20 {
        let (_, p) = files.remove(0);
        let _ = fs::remove_file(p);
    }
    Ok(())
}
