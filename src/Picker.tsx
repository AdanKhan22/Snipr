import { useEffect, useRef, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWebviewWindow } from "@tauri-apps/api/webviewWindow";
import { api, applyTheme } from "./api";
import type { Snippet } from "./types";

export default function Picker() {
  const [query, setQuery] = useState("");
  const [results, setResults] = useState<Snippet[]>([]);
  const [index, setIndex] = useState(0);
  const input = useRef<HTMLInputElement>(null);
  const list = useRef<HTMLDivElement>(null);

  useEffect(() => {
    api.getState().then((s) => applyTheme(s.settings.theme));
    const open = listen("picker-open", () => {
      setQuery("");
      setIndex(0);
      api.getState().then((s) => applyTheme(s.settings.theme));
      setTimeout(() => input.current?.focus(), 30);
    });
    const win = getCurrentWebviewWindow();
    const blur = win.onFocusChanged(({ payload: focused }) => {
      if (!focused) api.closePicker();
    });
    return () => {
      open.then((u) => u());
      blur.then((u) => u());
    };
  }, []);

  useEffect(() => {
    let live = true;
    api.search(query).then((r) => {
      if (live) {
        setResults(r);
        setIndex(0);
      }
    });
    return () => {
      live = false;
    };
  }, [query]);

  useEffect(() => {
    (list.current?.children[index] as HTMLElement | undefined)?.scrollIntoView({ block: "nearest" });
  }, [index]);

  function onKey(e: React.KeyboardEvent) {
    if (e.key === "ArrowDown") {
      e.preventDefault();
      setIndex((i) => Math.min(i + 1, results.length - 1));
    } else if (e.key === "ArrowUp") {
      e.preventDefault();
      setIndex((i) => Math.max(i - 1, 0));
    } else if (e.key === "Enter") {
      e.preventDefault();
      const s = results[index];
      if (s) api.pickSnippet(s.id);
    } else if (e.key === "Escape") {
      api.closePicker();
    }
  }

  return (
    <div className="picker" onKeyDown={onKey}>
      <input
        ref={input}
        autoFocus
        className="picker-input"
        placeholder="Search snippets by name, trigger or content…"
        value={query}
        onChange={(e) => setQuery(e.target.value)}
      />
      <div className="picker-list" ref={list}>
        {results.map((s, i) => (
          <div
            key={s.id}
            className={"picker-row" + (i === index ? " active" : "")}
            onMouseMove={() => setIndex(i)}
            onClick={() => api.pickSnippet(s.id)}
          >
            <div className="picker-title">
              <span>{s.name || s.trigger}</span>
              {s.trigger && <code>{s.trigger}</code>}
            </div>
            <div className="picker-preview">{s.content.replace(/\s+/g, " ").slice(0, 110)}</div>
          </div>
        ))}
        {results.length === 0 && <div className="empty">No snippets found</div>}
      </div>
      <div className="picker-foot">↑↓ navigate · Enter insert · Esc close</div>
    </div>
  );
}
