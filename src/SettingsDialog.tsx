import { useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import { api, applyTheme } from "./api";
import type { Settings } from "./types";

function HotkeyInput({ value, onChange }: { value: string; onChange: (v: string) => void }) {
  function onKey(e: React.KeyboardEvent) {
    e.preventDefault();
    if (["Control", "Shift", "Alt", "Meta"].includes(e.key)) return;
    if (e.key === "Backspace" || e.key === "Delete") return onChange("");
    const parts: string[] = [];
    if (e.ctrlKey) parts.push("Ctrl");
    if (e.altKey) parts.push("Alt");
    if (e.shiftKey) parts.push("Shift");
    if (e.metaKey) parts.push("Super");
    let key = e.code.startsWith("Key") ? e.code.slice(3) : e.code.startsWith("Digit") ? e.code.slice(5) : e.code;
    if (key === "Space") key = "Space";
    parts.push(key);
    if (parts.length >= 2) onChange(parts.join("+"));
  }
  return <input className="mono" readOnly value={value || "Not set"} onKeyDown={onKey} title="Click, then press the shortcut. Backspace clears." />;
}

interface Props {
  settings: Settings;
  libraryFile: string;
  version: string;
  onClose: () => void;
  onSaved: () => void;
}

export default function SettingsDialog({ settings, libraryFile, version, onClose, onSaved }: Props) {
  const [s, setS] = useState<Settings>(settings);
  const [error, setError] = useState("");
  const set = <K extends keyof Settings>(k: K, v: Settings[K]) => setS((x) => ({ ...x, [k]: v }));

  async function pickFolder() {
    const dir = await open({ directory: true, title: "Choose a sync folder (Dropbox, OneDrive, a Git repo…)" });
    if (typeof dir === "string") set("libraryPath", dir);
  }

  async function save() {
    try {
      await api.setSettings(s);
      applyTheme(s.theme);
      onSaved();
      onClose();
    } catch (e) {
      setError(String(e));
    }
  }

  return (
    <div className="modal-bg" onMouseDown={onClose}>
      <div className="modal" onMouseDown={(e) => e.stopPropagation()}>
        <h2>Settings</h2>

        <section>
          <h3>Hotkeys</h3>
          <div className="grid2">
            <div className="field">
              <label>Snippet picker</label>
              <HotkeyInput value={s.pickerHotkey} onChange={(v) => set("pickerHotkey", v)} />
            </div>
            <div className="field">
              <label>Pause / resume expansion</label>
              <HotkeyInput value={s.toggleHotkey} onChange={(v) => set("toggleHotkey", v)} />
            </div>
          </div>
        </section>

        <section>
          <h3>Expansion</h3>
          <div className="grid2">
            <div className="field">
              <label>Insert method</label>
              <select value={s.pasteMethod} onChange={(e) => set("pasteMethod", e.target.value as Settings["pasteMethod"])}>
                <option value="paste">Paste via clipboard (fast, best for long text)</option>
                <option value="type">Simulate typing (never touches clipboard)</option>
              </select>
            </div>
            <div className="field">
              <label>Delimiter characters</label>
              <input className="mono" value={s.delimiters} onChange={(e) => set("delimiters", e.target.value)} />
            </div>
          </div>
          <label className="check">
            <input type="checkbox" checked={s.restoreClipboard} onChange={(e) => set("restoreClipboard", e.target.checked)} />
            Restore clipboard after pasting
          </label>
          <label className="check">
            <input type="checkbox" checked={s.allowShell} onChange={(e) => set("allowShell", e.target.checked)} />
            Allow <code>#{"{shell:…}"}</code> variables to run PowerShell commands (only enable for libraries you trust)
          </label>
          <div className="field">
            <label>Never expand in these apps (comma separated)</label>
            <input
              placeholder="keepass.exe, game.exe"
              value={s.disabledApps.join(", ")}
              onChange={(e) =>
                set(
                  "disabledApps",
                  e.target.value
                    .split(",")
                    .map((x) => x.trim())
                    .filter(Boolean),
                )
              }
            />
          </div>
        </section>

        <section>
          <h3>Sync & storage</h3>
          <div className="field">
            <label>Library folder (leave empty for the default location)</label>
            <div className="row gap">
              <input
                value={s.libraryPath ?? ""}
                placeholder="Default (app data)"
                onChange={(e) => set("libraryPath", e.target.value || null)}
              />
              <button onClick={pickFolder}>Browse…</button>
            </div>
            <div className="muted small-text">
              Current file: <code>{libraryFile}</code>. Point this at a folder synced by Dropbox, OneDrive, Syncthing or Git; changes
              from other machines are picked up automatically.
            </div>
          </div>
        </section>

        <section>
          <h3>App</h3>
          <div className="row gap wrap">
            <label className="check">
              <input type="checkbox" checked={s.autostart} onChange={(e) => set("autostart", e.target.checked)} />
              Start with Windows
            </label>
            <label className="check">
              <input type="checkbox" checked={s.startHidden} onChange={(e) => set("startHidden", e.target.checked)} />
              Start minimised to tray
            </label>
            <div className="field inline">
              <label>Theme</label>
              <select value={s.theme} onChange={(e) => set("theme", e.target.value as Settings["theme"])}>
                <option value="system">System</option>
                <option value="light">Light</option>
                <option value="dark">Dark</option>
              </select>
            </div>
          </div>
        </section>

        {error && <div className="error">{error}</div>}
        <div className="row between">
          <span className="muted">Snipr v{version} · MIT licensed</span>
          <div className="row gap">
            <button onClick={onClose}>Cancel</button>
            <button className="primary" onClick={save}>
              Save settings
            </button>
          </div>
        </div>
      </div>
    </div>
  );
}
