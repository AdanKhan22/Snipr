export type MatchKind = "text" | "regex";
export type TriggerMode = "immediate" | "delimiter";
export type Boundary = "strict" | "loose";
export type AppRuleMode = "all" | "only" | "except";
export type PasteMethod = "paste" | "type";

export interface Snippet {
  id: string;
  name: string;
  trigger: string;
  content: string;
  groupId: string;
  enabled: boolean;
  kind: MatchKind;
  mode: TriggerMode;
  boundary: Boundary;
  caseSensitive: boolean;
  casePropagate: boolean;
  appMode: AppRuleMode;
  apps: string[];
  tags: string[];
  created: string;
  modified: string;
  useCount: number;
  lastUsed: string | null;
}

export interface Group {
  id: string;
  name: string;
  enabled: boolean;
}

export interface Library {
  version: number;
  groups: Group[];
  snippets: Snippet[];
  counters: Record<string, number>;
}

export interface Settings {
  pickerHotkey: string;
  toggleHotkey: string;
  pasteMethod: PasteMethod;
  restoreClipboard: boolean;
  allowShell: boolean;
  autostart: boolean;
  startHidden: boolean;
  libraryPath: string | null;
  disabledApps: string[];
  delimiters: string;
  theme: "system" | "light" | "dark";
}

export interface AppStateDto {
  library: Library;
  settings: Settings;
  enabled: boolean;
  supported: boolean;
  version: string;
  libraryFile: string;
}

export function newSnippet(groupId: string): Snippet {
  const now = new Date().toISOString();
  return {
    id: crypto.randomUUID(),
    name: "",
    trigger: "",
    content: "",
    groupId,
    enabled: true,
    kind: "text",
    mode: "immediate",
    boundary: "strict",
    caseSensitive: false,
    casePropagate: true,
    appMode: "all",
    apps: [],
    tags: [],
    created: now,
    modified: now,
    useCount: 0,
    lastUsed: null,
  };
}
