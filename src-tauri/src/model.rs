use serde::{Deserialize, Serialize};
use std::collections::HashMap;

pub fn new_id() -> String {
    uuid::Uuid::new_v4().to_string()
}

pub fn now() -> String {
    chrono::Utc::now().to_rfc3339()
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Default)]
#[serde(rename_all = "camelCase")]
pub enum MatchKind {
    #[default]
    Text,
    Regex,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Default)]
#[serde(rename_all = "camelCase")]
pub enum TriggerMode {
    /// Expand as soon as the last character of the trigger is typed.
    #[default]
    Immediate,
    /// Expand when a delimiter (space, punctuation) is typed after the trigger.
    Delimiter,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Default)]
#[serde(rename_all = "camelCase")]
pub enum Boundary {
    /// The trigger must start a word (previous char is not alphanumeric).
    #[default]
    Strict,
    /// The trigger may appear anywhere, even inside a word.
    Loose,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Default)]
#[serde(rename_all = "camelCase")]
pub enum AppRuleMode {
    #[default]
    All,
    Only,
    Except,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Default)]
#[serde(rename_all = "camelCase")]
pub enum PasteMethod {
    #[default]
    Paste,
    Type,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase", default)]
pub struct Snippet {
    pub id: String,
    pub name: String,
    pub trigger: String,
    pub content: String,
    pub group_id: String,
    pub enabled: bool,
    pub kind: MatchKind,
    pub mode: TriggerMode,
    pub boundary: Boundary,
    pub case_sensitive: bool,
    pub case_propagate: bool,
    pub app_mode: AppRuleMode,
    pub apps: Vec<String>,
    pub tags: Vec<String>,
    pub created: String,
    pub modified: String,
    pub use_count: u64,
    pub last_used: Option<String>,
}

impl Default for Snippet {
    fn default() -> Self {
        let t = now();
        Snippet {
            id: new_id(),
            name: String::new(),
            trigger: String::new(),
            content: String::new(),
            group_id: String::new(),
            enabled: true,
            kind: MatchKind::Text,
            mode: TriggerMode::Immediate,
            boundary: Boundary::Strict,
            case_sensitive: false,
            case_propagate: true,
            app_mode: AppRuleMode::All,
            apps: vec![],
            tags: vec![],
            created: t.clone(),
            modified: t,
            use_count: 0,
            last_used: None,
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase", default)]
pub struct Group {
    pub id: String,
    pub name: String,
    pub enabled: bool,
}

impl Default for Group {
    fn default() -> Self {
        Group { id: new_id(), name: "New group".into(), enabled: true }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase", default)]
pub struct Library {
    pub version: u32,
    pub groups: Vec<Group>,
    pub snippets: Vec<Snippet>,
    pub counters: HashMap<String, i64>,
}

impl Default for Library {
    fn default() -> Self {
        Library { version: 1, groups: vec![], snippets: vec![], counters: HashMap::new() }
    }
}

impl Library {
    /// A starter library shown on first launch.
    pub fn seeded() -> Library {
        let g = Group { id: new_id(), name: "General".into(), enabled: true };
        let mk = |name: &str, trigger: &str, content: &str| Snippet {
            name: name.into(),
            trigger: trigger.into(),
            content: content.into(),
            group_id: g.id.clone(),
            ..Snippet::default()
        };
        let snippets = vec![
            mk("Shrug", ";shrug", "¯\\_(ツ)_/¯"),
            mk("Today's date", ";date", "#{date:%Y-%m-%d}"),
            mk("Date in one week", ";nextweek", "#{date:%A, %d %B %Y|+1w}"),
            mk("Sign-off", ";sig", "Best regards,\n#{input:Your name}"),
            mk("Wrap clipboard in quotes", ";quote", "\"#{clipboard}\"#{cursor}"),
        ];
        Library { groups: vec![g], snippets, ..Library::default() }
    }
}

fn default_delimiters() -> String {
    " .,;:!?()[]{}\"'".to_string()
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase", default)]
pub struct Settings {
    pub picker_hotkey: String,
    pub toggle_hotkey: String,
    pub paste_method: PasteMethod,
    pub restore_clipboard: bool,
    pub sound_on_expand: bool,
    /// Type the text after `#{cursor}` instead of pasting it, so the caret lands exactly
    /// in place even in rich-text editors that reshape pasted line breaks.
    pub exact_cursor: bool,
    pub allow_shell: bool,
    pub autostart: bool,
    pub start_hidden: bool,
    /// Folder (or file) holding library.json — point it at Dropbox/Git/Syncthing to sync.
    pub library_path: Option<String>,
    pub disabled_apps: Vec<String>,
    pub delimiters: String,
    pub theme: String,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            picker_hotkey: "Ctrl+Alt+Space".into(),
            toggle_hotkey: "Ctrl+Alt+Shift+P".into(),
            paste_method: PasteMethod::Paste,
            restore_clipboard: true,
            sound_on_expand: true,
            exact_cursor: true,
            allow_shell: false,
            autostart: false,
            start_hidden: false,
            library_path: None,
            disabled_apps: vec![],
            delimiters: default_delimiters(),
            theme: "system".into(),
        }
    }
}
