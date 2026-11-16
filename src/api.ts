import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { useEffect, useState, useCallback } from "react";
import type { AppStateDto, Group, Settings, Snippet } from "./types";

export const api = {
  // The webview can finish loading before the backend's setup hook has registered
  // its state, so retry briefly instead of failing on startup.
  getState: async (): Promise<AppStateDto> => {
    for (let attempt = 0; ; attempt++) {
      try {
        return await invoke<AppStateDto>("get_state");
      } catch (e) {
        if (attempt >= 40 || !String(e).includes("not managed")) throw e;
        await new Promise((r) => setTimeout(r, 150));
      }
    }
  },
  saveSnippet: (snippet: Snippet) => invoke<void>("save_snippet", { snippet }),
  deleteSnippets: (ids: string[]) => invoke<void>("delete_snippets", { ids }),
  moveSnippets: (ids: string[], groupId: string) => invoke<void>("move_snippets", { ids, groupId }),
  saveGroup: (group: Group) => invoke<void>("save_group", { group }),
  deleteGroup: (id: string) => invoke<void>("delete_group", { id }),
  setSettings: (settings: Settings) => invoke<void>("set_settings", { settings }),
  setEnabled: (enabled: boolean) => invoke<void>("set_enabled", { enabled }),
  preview: (content: string) => invoke<string>("preview_snippet", { content }),
  search: (query: string) => invoke<Snippet[]>("search_snippets", { query }),
  closePicker: () => invoke<void>("close_picker"),
  pickSnippet: (id: string) => invoke<void>("pick_snippet", { id }),
  submitPrompt: (value: string | null) => invoke<void>("submit_prompt", { value }),
  importFile: (path: string) => invoke<number>("import_file", { path }),
  exportFile: (path: string) => invoke<void>("export_file", { path }),
};

/** Loads app state and keeps it fresh when the backend reports changes. */
export function useAppState() {
  const [state, setState] = useState<AppStateDto | null>(null);
  const [error, setError] = useState<string>("");
  const refresh = useCallback(
    () =>
      api
        .getState()
        .then((s) => {
          setError("");
          setState(s);
        })
        .catch((e) => {
          console.error(e);
          setError(String(e));
        }),
    [],
  );

  useEffect(() => {
    refresh();
    const subs = [listen("library-changed", refresh), listen("enabled-changed", refresh)];
    return () => {
      subs.forEach((p) => p.then((un) => un()));
    };
  }, [refresh]);

  return { state, refresh, error };
}

export function applyTheme(theme: string) {
  const root = document.documentElement;
  if (theme === "light" || theme === "dark") root.dataset.theme = theme;
  else delete root.dataset.theme;
}
