# Snipr

An open-source text expander for Windows, built as a modern successor to [Beeftext](https://github.com/xmichelo/Beeftext) (no longer maintained).
Type a short trigger like `;sig` in any app and it expands into a full snippet.

Built with **Tauri 2** (Rust core + React UI). Small, fast, and structured so macOS/Linux backends can be added.

## Features

**Everything Beeftext did**
- Snippets organised in groups, enable/disable per snippet or group
- Global trigger detection in any application (low-level keyboard hook)
- Variables: date/time, clipboard, cursor position, input prompts, key presses, nested snippets
- Import / export (Beeftext JSON import is supported)
- Tray icon, start with Windows, pause hotkey

**Beyond Beeftext**
- **Fuzzy-search picker** (`Ctrl+Alt+Space`): find any snippet by name, trigger or content and insert it
- **Regex triggers** with capture groups (`#{1}`, `#{2}`)
- **Per-app rules**: run a snippet only in (or never in) chosen apps; global "never expand in" list
- **Rich variables**: date math (`#{date:%Y-%m-%d|+1w}`), `#{upper:…}`/`lower`/`title`/`trim`, `#{random:1-100}`, `#{counter:name}`, `#{uuid}`, `#{env:NAME}`, `#{key:ctrl+shift+left}`, opt-in `#{shell:…}`; all nestable
- **Case matching**: typing `;SIG` or `;Sig` adapts the expansion
- **Expansion modes**: immediately, or after a delimiter (space/punctuation); strict or loose word boundary
- **Live preview** of the rendered snippet in the editor
- **Sync anywhere**: point the library at a Dropbox/OneDrive/Syncthing/Git folder; external changes are picked up live
- **Automatic backups** (rolling, last 20) and atomic saves
- **Statistics**: usage counts, characters and time saved
- Clipboard is restored after pasting (text and images); or choose "simulate typing" to never touch it
- CSV import, dark/light theme

## Variables

| Variable | Result |
| --- | --- |
| `#{cursor}` | Caret ends up here |
| `#{clipboard}` | Clipboard text |
| `#{date}` `#{time}` `#{datetime}` | Optional strftime format, e.g. `#{date:%d/%m/%Y}`; Beeftext/Qt formats (`yyyy-MM-dd`) also work. Offset after `\|`: `+1w`, `-2d`, `+1M-1d` |
| `#{input:Label\|default}` | Prompts you for a value |
| `#{snippet:name or trigger}` | Inserts another snippet |
| `#{key:enter}` | Presses a key or chord (`tab*3`, `ctrl+a`) |
| `#{upper:…}` `#{lower:…}` `#{title:…}` `#{trim:…}` | Text transforms |
| `#{random:1-100}` `#{counter:name}` `#{uuid}` | Numbers and ids |
| `#{env:NAME}` | Environment variable |
| `#{1}` `#{match}` | Regex capture groups |
| `#{shell:cmd}` | PowerShell output (must be enabled in Settings) |
| `##{` | A literal `#{` |

## Building

Requirements: [Rust](https://rustup.rs) (MSVC toolchain + Visual Studio Build Tools "Desktop development with C++"), Node.js 20+, WebView2 (included in Windows 11).

```bash
npm install
npm run tauri dev      # development
npm run tauri build    # installer in src-tauri/target/release/bundle
```

Run the Rust tests with `cargo test` inside `src-tauri/`.

## Architecture

```
src-tauri/src/
  os/        OS layer: keyboard/mouse hooks, SendInput, focus, active app (win.rs; stub.rs elsewhere)
  engine.rs  typed-character buffer, trigger matching, expansion + injection
  variables.rs  #{…} renderer
  store.rs / model.rs / import.rs   JSON library, settings, backups, importers
  commands.rs / lib.rs   Tauri commands, tray, hotkeys, sync watcher
src/         React UI (main window, picker, input prompt)
```

Adding another OS means implementing the functions in `os/stub.rs` (listener, injection, foreground app, focus).

## Notes & limitations

- Some elevated (run-as-administrator) windows cannot be typed into by a non-elevated app. Run Snipr as admin if you need that.
- Password fields can't be detected: add password managers to *Never expand in these apps*.
- The Beeftext importer is best effort based on its documented JSON structure; please open an issue with a sample if your export doesn't import.

## License

MIT
