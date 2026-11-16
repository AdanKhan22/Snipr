import { useEffect, useMemo, useState } from "react";
import { open, save as saveDialog } from "@tauri-apps/plugin-dialog";
import { api, applyTheme, useAppState } from "./api";
import { newSnippet, type Snippet } from "./types";
import Editor from "./Editor";
import SettingsDialog from "./SettingsDialog";

type View = { kind: "group"; id: string } | { kind: "all" } | { kind: "stats" };

export default function App() {
  const { state, refresh } = useAppState();
  const [view, setView] = useState<View>({ kind: "all" });
  const [query, setQuery] = useState("");
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [draftNew, setDraftNew] = useState<Snippet | null>(null);
  const [showSettings, setShowSettings] = useState(false);
  const [toast, setToast] = useState("");
  const [renaming, setRenaming] = useState<string | null>(null);

  useEffect(() => {
    if (state) applyTheme(state.settings.theme);
  }, [state?.settings.theme]); // eslint-disable-line react-hooks/exhaustive-deps

  const lib = state?.library;

  const visible = useMemo(() => {
    if (!lib) return [];
    const q = query.trim().toLowerCase();
    return lib.snippets
      .filter((s) => view.kind === "all" || (view.kind === "group" && s.groupId === view.id))
      .filter(
        (s) =>
          !q ||
          s.name.toLowerCase().includes(q) ||
          s.trigger.toLowerCase().includes(q) ||
          s.content.toLowerCase().includes(q) ||
          s.tags.some((t) => t.toLowerCase().includes(q)),
      )
      .sort((a, b) => (a.name || a.trigger).localeCompare(b.name || b.trigger));
  }, [lib, view, query]);

  if (!state || !lib) return <div className="loading">Loading…</div>;

  const flash = (m: string) => {
    setToast(m);
    setTimeout(() => setToast(""), 3000);
  };

  const current = draftNew ?? lib.snippets.find((s) => s.id === selectedId) ?? null;
  const defaultGroup = view.kind === "group" ? view.id : lib.groups[0]?.id ?? "";

  function startNew() {
    const s = newSnippet(defaultGroup);
    setDraftNew(s);
    setSelectedId(null);
    if (view.kind === "stats") setView({ kind: "all" });
  }

  async function importFile() {
    const path = await open({
      title: "Import snippets",
      filters: [{ name: "Beeftext / Snipr JSON, CSV", extensions: ["json", "csv"] }],
    });
    if (typeof path !== "string") return;
    try {
      const n = await api.importFile(path);
      flash(`Imported ${n} snippet${n === 1 ? "" : "s"}`);
      refresh();
    } catch (e) {
      flash(`Import failed: ${e}`);
    }
  }

  async function exportFile() {
    const path = await saveDialog({ title: "Export library", defaultPath: "snipr-library.json", filters: [{ name: "JSON", extensions: ["json"] }] });
    if (!path) return;
    try {
      await api.exportFile(path);
      flash("Library exported");
    } catch (e) {
      flash(`Export failed: ${e}`);
    }
  }

  async function addGroup() {
    const id = crypto.randomUUID();
    await api.saveGroup({ id, name: "New group", enabled: true });
    await refresh();
    setView({ kind: "group", id });
    setRenaming(id);
  }

  const groupCount = (id: string) => lib.snippets.filter((s) => s.groupId === id).length;

  return (
    <div className="app">
      <aside className="sidebar">
        <div className="brand">
          <span className="logo">C</span> Snipr
        </div>

        <button className={"pill " + (state.enabled ? "on" : "off")} onClick={() => api.setEnabled(!state.enabled)}>
          <span className="dot" />
          {state.enabled ? "Expansion active" : "Paused"}
        </button>
        {!state.supported && <div className="warn">Keyboard expansion isn't supported on this OS yet.</div>}

        <nav>
          <div className={"nav-item" + (view.kind === "all" ? " active" : "")} onClick={() => setView({ kind: "all" })}>
            <span>All snippets</span>
            <em>{lib.snippets.length}</em>
          </div>
          <div className="nav-label">
            Groups <button className="icon" title="New group" onClick={addGroup}>＋</button>
          </div>
          {lib.groups.map((g) => (
            <div
              key={g.id}
              className={"nav-item" + (view.kind === "group" && view.id === g.id ? " active" : "")}
              onClick={() => setView({ kind: "group", id: g.id })}
              onDoubleClick={() => setRenaming(g.id)}
            >
              {renaming === g.id ? (
                <input
                  autoFocus
                  defaultValue={g.name}
                  onClick={(e) => e.stopPropagation()}
                  onBlur={async (e) => {
                    await api.saveGroup({ ...g, name: e.target.value.trim() || g.name });
                    setRenaming(null);
                  }}
                  onKeyDown={(e) => e.key === "Enter" && (e.target as HTMLInputElement).blur()}
                />
              ) : (
                <span className={g.enabled ? "" : "strike"}>{g.name}</span>
              )}
              <em>{groupCount(g.id)}</em>
            </div>
          ))}
          {view.kind === "group" &&
            (() => {
              const g = lib.groups.find((x) => x.id === view.id);
              if (!g) return null;
              return (
                <div className="group-tools">
                  <button className="small" onClick={() => api.saveGroup({ ...g, enabled: !g.enabled })}>
                    {g.enabled ? "Disable group" : "Enable group"}
                  </button>
                  <button className="small" onClick={() => setRenaming(g.id)}>
                    Rename
                  </button>
                  <button
                    className="small danger"
                    onClick={() => {
                      if (confirm(`Delete group "${g.name}" and its ${groupCount(g.id)} snippets?`)) {
                        api.deleteGroup(g.id);
                        setView({ kind: "all" });
                      }
                    }}
                  >
                    Delete
                  </button>
                </div>
              );
            })()}
          <div className={"nav-item" + (view.kind === "stats" ? " active" : "")} onClick={() => setView({ kind: "stats" })}>
            <span>Statistics</span>
          </div>
        </nav>

        <div className="sidebar-foot">
          <button onClick={importFile}>Import…</button>
          <button onClick={exportFile}>Export…</button>
          <button onClick={() => setShowSettings(true)}>Settings</button>
        </div>
      </aside>

      {view.kind === "stats" ? (
        <Stats snippets={lib.snippets} />
      ) : (
        <>
          <section className="list">
            <div className="list-head">
              <input placeholder="Search…" value={query} onChange={(e) => setQuery(e.target.value)} />
              <button className="primary" onClick={startNew}>
                ＋ New
              </button>
            </div>
            <div className="rows">
              {visible.map((s) => (
                <div
                  key={s.id}
                  className={"row-item" + (s.id === selectedId && !draftNew ? " active" : "") + (s.enabled ? "" : " disabled")}
                  onClick={() => {
                    setSelectedId(s.id);
                    setDraftNew(null);
                  }}
                >
                  <div className="row-title">
                    <span>{s.name || s.trigger}</span>
                    <code>{s.trigger}</code>
                  </div>
                  <div className="row-sub">{s.content.replace(/\s+/g, " ").slice(0, 90)}</div>
                </div>
              ))}
              {visible.length === 0 && <div className="empty">{query ? "No matches" : "No snippets yet. Click “New”."}</div>}
            </div>
          </section>

          <main className="detail">
            {current ? (
              <Editor
                key={current.id}
                snippet={current}
                groups={lib.groups}
                isNew={!!draftNew}
                onSave={async (s) => {
                  await api.saveSnippet(s);
                  setDraftNew(null);
                  setSelectedId(s.id);
                  await refresh();
                }}
                onDelete={async () => {
                  if (confirm(`Delete "${current.name || current.trigger}"?`)) {
                    await api.deleteSnippets([current.id]);
                    setSelectedId(null);
                  }
                }}
                onDuplicate={() => {
                  const copy = { ...newSnippet(current.groupId), ...current, id: crypto.randomUUID(), name: `${current.name} copy`, useCount: 0, lastUsed: null };
                  setDraftNew(copy);
                }}
              />
            ) : (
              <div className="placeholder">
                <h2>Select a snippet or create a new one</h2>
                <p>
                  Type a trigger like <code>;sig</code> anywhere in Windows and it expands automatically. Press{" "}
                  <kbd>{state.settings.pickerHotkey}</kbd> for the fuzzy-search picker.
                </p>
              </div>
            )}
          </main>
        </>
      )}

      {showSettings && (
        <SettingsDialog
          settings={state.settings}
          libraryFile={state.libraryFile}
          version={state.version}
          onClose={() => setShowSettings(false)}
          onSaved={refresh}
        />
      )}
      {toast && <div className="toast">{toast}</div>}
    </div>
  );
}

function Stats({ snippets }: { snippets: Snippet[] }) {
  const total = snippets.reduce((a, s) => a + s.useCount, 0);
  const chars = snippets.reduce((a, s) => a + s.useCount * Math.max(0, s.content.length - s.trigger.length), 0);
  const minutes = Math.round(chars / 200); // ~40 words/minute typing
  const top = [...snippets].filter((s) => s.useCount > 0).sort((a, b) => b.useCount - a.useCount).slice(0, 10);
  return (
    <main className="detail stats">
      <h2>Statistics</h2>
      <div className="cards">
        <div className="card">
          <b>{total}</b>
          <span>expansions</span>
        </div>
        <div className="card">
          <b>{chars.toLocaleString()}</b>
          <span>characters saved</span>
        </div>
        <div className="card">
          <b>{minutes}</b>
          <span>minutes saved (est.)</span>
        </div>
        <div className="card">
          <b>{snippets.length}</b>
          <span>snippets</span>
        </div>
      </div>
      <h3>Most used</h3>
      {top.length === 0 && <div className="muted">Nothing used yet.</div>}
      {top.map((s) => (
        <div key={s.id} className="bar-row">
          <span>{s.name || s.trigger}</span>
          <div className="bar">
            <div style={{ width: `${(s.useCount / top[0].useCount) * 100}%` }} />
          </div>
          <em>{s.useCount}</em>
        </div>
      ))}
    </main>
  );
}
