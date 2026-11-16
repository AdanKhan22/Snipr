import { useEffect, useRef, useState } from "react";
import { api } from "./api";
import type { Group, Snippet } from "./types";

const VARIABLES: { label: string; text: string; hint: string }[] = [
  { label: "Cursor position", text: "#{cursor}", hint: "Place the caret here after expansion" },
  { label: "Clipboard", text: "#{clipboard}", hint: "Current clipboard text" },
  { label: "Date", text: "#{date:%Y-%m-%d}", hint: "strftime format; add |+7d for date math" },
  { label: "Date + offset", text: "#{date:%A %d %B|+1w}", hint: "Offsets: s m h d w M y, e.g. +1M-2d" },
  { label: "Time", text: "#{time:%H:%M}", hint: "Current time" },
  { label: "Ask for input", text: "#{input:Label|default}", hint: "Shows a prompt window" },
  { label: "Other snippet", text: "#{snippet:name or trigger}", hint: "Nest another snippet" },
  { label: "Key press", text: "#{key:enter}", hint: "enter, tab, ctrl+shift+left, tab*3 …" },
  { label: "UPPERCASE", text: "#{upper:text}", hint: "Also lower, title, trim" },
  { label: "Counter", text: "#{counter:invoice}", hint: "Increments on every use" },
  { label: "Random number", text: "#{random:1-100}", hint: "Inclusive range" },
  { label: "UUID", text: "#{uuid}", hint: "Random unique id" },
  { label: "Environment variable", text: "#{env:USERNAME}", hint: "Read an env var" },
  { label: "Regex group 1", text: "#{1}", hint: "Capture group of a regex trigger" },
  { label: "Shell command", text: "#{shell:Get-Date}", hint: "PowerShell output (enable in Settings)" },
];

interface Props {
  snippet: Snippet;
  groups: Group[];
  isNew: boolean;
  onSave: (s: Snippet) => Promise<void>;
  onDelete: () => void;
  onDuplicate: () => void;
}

export default function Editor({ snippet, groups, isNew, onSave, onDelete, onDuplicate }: Props) {
  const [draft, setDraft] = useState<Snippet>(snippet);
  const [preview, setPreview] = useState("");
  const [error, setError] = useState("");
  const [menu, setMenu] = useState(false);
  const area = useRef<HTMLTextAreaElement>(null);

  useEffect(() => {
    setDraft(snippet);
    setError("");
  }, [snippet.id]); // eslint-disable-line react-hooks/exhaustive-deps

  useEffect(() => {
    const t = setTimeout(() => api.preview(draft.content).then(setPreview), 150);
    return () => clearTimeout(t);
  }, [draft.content]);

  const dirty = JSON.stringify({ ...draft, modified: "" }) !== JSON.stringify({ ...snippet, modified: "" });
  const set = <K extends keyof Snippet>(k: K, v: Snippet[K]) => setDraft((d) => ({ ...d, [k]: v }));

  async function save() {
    if (!draft.trigger.trim()) return setError("A trigger is required.");
    try {
      await onSave({ ...draft, name: draft.name.trim() || draft.trigger });
      setError("");
    } catch (e) {
      setError(String(e));
    }
  }

  useEffect(() => {
    const h = (e: KeyboardEvent) => {
      if ((e.ctrlKey || e.metaKey) && e.key === "s") {
        e.preventDefault();
        save();
      }
    };
    window.addEventListener("keydown", h);
    return () => window.removeEventListener("keydown", h);
  });

  function insertVar(text: string) {
    const el = area.current;
    const start = el?.selectionStart ?? draft.content.length;
    const end = el?.selectionEnd ?? start;
    set("content", draft.content.slice(0, start) + text + draft.content.slice(end));
    setMenu(false);
    setTimeout(() => {
      el?.focus();
      el?.setSelectionRange(start + text.length, start + text.length);
    }, 0);
  }

  const regex = draft.kind === "regex";

  return (
    <div className="editor">
      <div className="editor-head">
        <input
          className="title-input"
          placeholder="Snippet name"
          value={draft.name}
          onChange={(e) => set("name", e.target.value)}
        />
        <label className="switch" title="Enabled">
          <input type="checkbox" checked={draft.enabled} onChange={(e) => set("enabled", e.target.checked)} />
          <span>Enabled</span>
        </label>
      </div>

      <div className="grid2">
        <div className="field">
          <label>{regex ? "Trigger (regex, matched at end of typed text)" : "Trigger"}</label>
          <input
            className="mono"
            placeholder={regex ? "(\\d+)x(\\d+)" : ";sig"}
            value={draft.trigger}
            onChange={(e) => set("trigger", e.target.value)}
          />
        </div>
        <div className="field">
          <label>Group</label>
          <select value={draft.groupId} onChange={(e) => set("groupId", e.target.value)}>
            {groups.map((g) => (
              <option key={g.id} value={g.id}>
                {g.name}
              </option>
            ))}
          </select>
        </div>
      </div>

      <div className="field">
        <div className="row between">
          <label>Content</label>
          <div className="menu-wrap">
            <button className="small" onClick={() => setMenu((m) => !m)}>
              Insert variable ▾
            </button>
            {menu && (
              <div className="menu" onMouseLeave={() => setMenu(false)}>
                {VARIABLES.map((v) => (
                  <div key={v.label} className="menu-item" onClick={() => insertVar(v.text)} title={v.hint}>
                    <span>{v.label}</span>
                    <code>{v.text}</code>
                  </div>
                ))}
              </div>
            )}
          </div>
        </div>
        <textarea
          ref={area}
          className="mono content"
          spellCheck={false}
          placeholder="What should this expand to?"
          value={draft.content}
          onChange={(e) => set("content", e.target.value)}
        />
      </div>

      <div className="field">
        <label>Preview</label>
        <pre className="preview">{preview || " "}</pre>
      </div>

      <details className="advanced">
        <summary>Matching & app rules</summary>
        <div className="grid3">
          <div className="field">
            <label>Trigger type</label>
            <select value={draft.kind} onChange={(e) => set("kind", e.target.value as Snippet["kind"])}>
              <option value="text">Plain text</option>
              <option value="regex">Regular expression</option>
            </select>
          </div>
          <div className="field">
            <label>Expand</label>
            <select value={draft.mode} onChange={(e) => set("mode", e.target.value as Snippet["mode"])}>
              <option value="immediate">Immediately when typed</option>
              <option value="delimiter">After a space / punctuation</option>
            </select>
          </div>
          <div className="field">
            <label>Word boundary</label>
            <select
              value={draft.boundary}
              disabled={regex}
              onChange={(e) => set("boundary", e.target.value as Snippet["boundary"])}
            >
              <option value="strict">Strict (start of a word)</option>
              <option value="loose">Loose (anywhere)</option>
            </select>
          </div>
        </div>
        <div className="row gap">
          <label className="check">
            <input type="checkbox" checked={draft.caseSensitive} onChange={(e) => set("caseSensitive", e.target.checked)} />
            Case sensitive
          </label>
          <label className="check" title="Typing the trigger in CAPS or Capitalised adapts the expansion">
            <input
              type="checkbox"
              checked={draft.casePropagate}
              disabled={draft.caseSensitive || regex}
              onChange={(e) => set("casePropagate", e.target.checked)}
            />
            Match typed capitalisation
          </label>
        </div>
        <div className="grid2">
          <div className="field">
            <label>Applications</label>
            <select value={draft.appMode} onChange={(e) => set("appMode", e.target.value as Snippet["appMode"])}>
              <option value="all">Work everywhere</option>
              <option value="only">Only in these apps</option>
              <option value="except">Everywhere except these apps</option>
            </select>
          </div>
          <div className="field">
            <label>App executables (comma separated)</label>
            <input
              disabled={draft.appMode === "all"}
              placeholder="chrome.exe, code.exe"
              value={draft.apps.join(", ")}
              onChange={(e) =>
                set(
                  "apps",
                  e.target.value
                    .split(",")
                    .map((x) => x.trim())
                    .filter(Boolean),
                )
              }
            />
          </div>
        </div>
        <div className="field">
          <label>Tags (comma separated)</label>
          <input
            value={draft.tags.join(", ")}
            onChange={(e) =>
              set(
                "tags",
                e.target.value
                  .split(",")
                  .map((x) => x.trim())
                  .filter(Boolean),
              )
            }
          />
        </div>
      </details>

      {error && <div className="error">{error}</div>}

      <div className="editor-foot">
        <div className="muted">{isNew ? "New snippet" : `Used ${draft.useCount}×`}</div>
        <div className="row gap">
          {!isNew && <button onClick={onDuplicate}>Duplicate</button>}
          {!isNew && (
            <button className="danger" onClick={onDelete}>
              Delete
            </button>
          )}
          <button className="primary" disabled={!dirty && !isNew} onClick={save}>
            Save{dirty ? " •" : ""}
          </button>
        </div>
      </div>
    </div>
  );
}
