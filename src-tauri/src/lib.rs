mod commands;
mod engine;
mod import;
mod model;
mod os;
mod store;
mod variables;

use commands::AppState;
use parking_lot::Mutex;
use std::sync::atomic::AtomicIsize;
use std::sync::Arc;
use std::time::Duration;
use tauri::menu::{Menu, MenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Emitter, Manager, WindowEvent};
use tauri_plugin_autostart::ManagerExt as _;
use tauri_plugin_global_shortcut::{GlobalShortcutExt, Shortcut, ShortcutState};

static TOGGLE_ITEM: Mutex<Option<MenuItem<tauri::Wry>>> = Mutex::new(None);

fn show_main(app: &AppHandle) {
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.show();
        let _ = w.unminimize();
        let _ = w.set_focus();
    }
}

pub fn set_enabled_everywhere(app: &AppHandle, enabled: bool) {
    let st = app.state::<Arc<AppState>>();
    st.engine.set_enabled(enabled);
    if let Some(item) = TOGGLE_ITEM.lock().as_ref() {
        let _ = item.set_text(if enabled { "Pause expansion" } else { "Resume expansion" });
    }
    let _ = app.emit("enabled-changed", enabled);
}

/// Re-register global hotkeys and apply autostart from the current settings.
pub fn apply_system_settings(app: &AppHandle) {
    let st = app.state::<Arc<AppState>>();
    let settings = st.settings.lock().clone();

    let gs = app.global_shortcut();
    let _ = gs.unregister_all();
    for hk in [&settings.picker_hotkey, &settings.toggle_hotkey] {
        if let Ok(sc) = hk.parse::<Shortcut>() {
            if let Err(e) = gs.register(sc) {
                eprintln!("snipr: could not register hotkey {hk}: {e}");
            }
        }
    }

    let auto = app.autolaunch();
    let _ = if settings.autostart { auto.enable() } else { auto.disable() };
}

fn on_hotkey(app: &AppHandle, shortcut: &Shortcut) {
    let st = app.state::<Arc<AppState>>();
    let settings = st.settings.lock().clone();
    if settings.picker_hotkey.parse::<Shortcut>().ok().as_ref() == Some(shortcut) {
        commands::open_picker(app);
    } else if settings.toggle_hotkey.parse::<Shortcut>().ok().as_ref() == Some(shortcut) {
        let now = st.engine.is_enabled();
        set_enabled_everywhere(app, !now);
    }
}

pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| show_main(app)))
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_autostart::init(tauri_plugin_autostart::MacosLauncher::LaunchAgent, Some(vec!["--hidden"])))
        .plugin(
            tauri_plugin_global_shortcut::Builder::new()
                .with_handler(|app, shortcut, event| {
                    if event.state() == ShortcutState::Pressed {
                        on_hotkey(app, shortcut);
                    }
                })
                .build(),
        )
        .setup(|app| {
            let handle = app.handle().clone();
            let dir = app.path().app_data_dir().expect("app data dir");
            std::fs::create_dir_all(&dir).ok();

            let settings = store::load_settings(&dir);
            let lib_path = store::library_path(&dir, &settings);
            let library = store::load_library(&lib_path);
            if !lib_path.exists() {
                let _ = store::save_library(&lib_path, &library, &store::backups_dir(&dir));
                let _ = store::save_settings(&dir, &settings);
            }

            let engine = engine::Engine::new(handle.clone());
            engine.reload(&library, &settings);
            engine.start();

            let state = Arc::new(AppState {
                dir,
                library: Mutex::new(library),
                settings: Mutex::new(settings.clone()),
                engine,
                prev_focus: AtomicIsize::new(0),
                last_mtime: Mutex::new(store::mtime(&lib_path)),
            });
            app.manage(state.clone());

            // --- tray
            let open = MenuItem::with_id(app, "open", "Open Snipr", true, None::<&str>)?;
            let toggle = MenuItem::with_id(app, "toggle", "Pause expansion", true, None::<&str>)?;
            let quit = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;
            let menu = Menu::with_items(app, &[&open, &toggle, &quit])?;
            *TOGGLE_ITEM.lock() = Some(toggle);
            TrayIconBuilder::with_id("tray")
                .icon(app.default_window_icon().unwrap().clone())
                .tooltip("Snipr")
                .menu(&menu)
                .show_menu_on_left_click(false)
                .on_menu_event(|app, event| match event.id.as_ref() {
                    "open" => show_main(app),
                    "toggle" => {
                        let now = app.state::<Arc<AppState>>().engine.is_enabled();
                        set_enabled_everywhere(app, !now);
                    }
                    "quit" => app.exit(0),
                    _ => {}
                })
                .on_tray_icon_event(|tray, event| {
                    if let TrayIconEvent::Click { button: MouseButton::Left, button_state: MouseButtonState::Up, .. } = event {
                        show_main(tray.app_handle());
                    }
                })
                .build(app)?;

            apply_system_settings(&handle);

            let hidden = settings.start_hidden || std::env::args().any(|a| a == "--hidden");
            if !hidden {
                show_main(&handle);
            }

            // --- pick up external edits to the library file (Dropbox / Git / Syncthing sync)
            std::thread::spawn(move || loop {
                std::thread::sleep(Duration::from_secs(2));
                let st = handle.state::<Arc<AppState>>();
                let settings = st.settings.lock().clone();
                let path = store::library_path(&st.dir, &settings);
                let m = store::mtime(&path);
                if m.is_some() && m != *st.last_mtime.lock() {
                    *st.last_mtime.lock() = m;
                    let lib = store::load_library(&path);
                    st.engine.reload(&lib, &settings);
                    *st.library.lock() = lib;
                    let _ = handle.emit("library-changed", ());
                }
            });
            Ok(())
        })
        .on_window_event(|window, event| {
            if let WindowEvent::CloseRequested { api, .. } = event {
                // Closing any window only hides it; the app lives in the tray.
                api.prevent_close();
                let _ = window.hide();
            }
        })
        .invoke_handler(tauri::generate_handler![
            commands::get_state,
            commands::save_snippet,
            commands::delete_snippets,
            commands::move_snippets,
            commands::save_group,
            commands::delete_group,
            commands::set_settings,
            commands::set_enabled,
            commands::preview_snippet,
            commands::search_snippets,
            commands::close_picker,
            commands::pick_snippet,
            commands::submit_prompt,
            commands::import_file,
            commands::export_file,
        ])
        .run(tauri::generate_context!())
        .expect("error while running Snipr");
}
