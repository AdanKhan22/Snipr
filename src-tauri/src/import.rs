//! Importers: native Snipr JSON, Beeftext JSON (best effort), and CSV (trigger,content[,name]).

use crate::model::*;
use serde_json::Value;

fn group_index(lib: &mut Library, name: &str) -> String {
    if let Some(g) = lib.groups.iter().find(|g| g.name.eq_ignore_ascii_case(name)) {
        return g.id.clone();
    }
    let g = Group { name: name.to_string(), ..Group::default() };
    let id = g.id.clone();
    lib.groups.push(g);
    id
}

fn str_of<'a>(v: &'a Value, keys: &[&str]) -> Option<&'a str> {
    keys.iter().find_map(|k| v.get(*k).and_then(|x| x.as_str()))
}

pub fn import_json(text: &str, lib: &mut Library) -> Result<usize, String> {
    let v: Value = serde_json::from_str(text).map_err(|e| format!("Not valid JSON: {e}"))?;

    // Native Snipr export
    if v.get("snippets").is_some_and(|s| s.is_array()) {
        let incoming: Library = serde_json::from_value(v).map_err(|e| e.to_string())?;
        let mut gmap = std::collections::HashMap::new();
        for g in &incoming.groups {
            let id = group_index(lib, &g.name);
            gmap.insert(g.id.clone(), id);
        }
        let mut n = 0;
        for mut s in incoming.snippets {
            if lib.snippets.iter().any(|x| x.id == s.id) {
                continue;
            }
            s.group_id = gmap.get(&s.group_id).cloned().unwrap_or_else(|| group_index(lib, "Imported"));
            lib.snippets.push(s);
            n += 1;
        }
        return Ok(n);
    }

    // Beeftext: { "groups": [ { "name", "combos": [ { "keyword", "snippet", ... } ] } ] }
    let groups: Vec<Value> = if let Some(g) = v.get("groups").and_then(|g| g.as_array()) {
        g.clone()
    } else if v.is_array() {
        vec![serde_json::json!({ "name": "Imported", "combos": v })]
    } else if v.get("combos").is_some() {
        vec![v.clone()]
    } else {
        return Err("Unrecognised file format".into());
    };

    let mut n = 0;
    for g in groups {
        let gname = str_of(&g, &["name"]).unwrap_or("Imported").to_string();
        let gid = group_index(lib, &gname);
        if g.get("enabled").and_then(|e| e.as_bool()) == Some(false) {
            if let Some(grp) = lib.groups.iter_mut().find(|x| x.id == gid) {
                grp.enabled = false;
            }
        }
        let combos = g.get("combos").and_then(|c| c.as_array()).cloned().unwrap_or_default();
        for c in combos {
            let trigger = str_of(&c, &["keyword", "trigger"]).unwrap_or("").to_string();
            let content = str_of(&c, &["snippet", "content", "text"]).unwrap_or("").to_string();
            if trigger.is_empty() && content.is_empty() {
                continue;
            }
            let name = str_of(&c, &["name", "description"]).unwrap_or(&trigger).to_string();
            let mode = c.get("matchingMode").map(|m| m.to_string().to_lowercase()).unwrap_or_default();
            let case = c.get("caseSensitivity").map(|m| m.to_string().to_lowercase()).unwrap_or_default();
            lib.snippets.push(Snippet {
                name,
                trigger,
                content,
                group_id: gid.clone(),
                enabled: c.get("enabled").and_then(|e| e.as_bool()).unwrap_or(true),
                boundary: if mode.contains("loose") { Boundary::Loose } else { Boundary::Strict },
                case_sensitive: case.contains("sensitive") && !case.contains("insensitive"),
                ..Snippet::default()
            });
            n += 1;
        }
    }
    Ok(n)
}

pub fn import_csv(text: &str, lib: &mut Library) -> Result<usize, String> {
    let rows = parse_csv(text);
    let gid = group_index(lib, "Imported");
    let mut n = 0;
    for (i, row) in rows.iter().enumerate() {
        if row.len() < 2 {
            continue;
        }
        if i == 0 && row[0].eq_ignore_ascii_case("trigger") {
            continue; // header
        }
        lib.snippets.push(Snippet {
            trigger: row[0].clone(),
            content: row[1].clone(),
            name: row.get(2).cloned().filter(|s| !s.is_empty()).unwrap_or_else(|| row[0].clone()),
            group_id: gid.clone(),
            ..Snippet::default()
        });
        n += 1;
    }
    if n == 0 {
        return Err("No rows found (expected: trigger,content[,name])".into());
    }
    Ok(n)
}

fn parse_csv(text: &str) -> Vec<Vec<String>> {
    let mut rows = vec![];
    let mut row = vec![];
    let mut field = String::new();
    let mut quoted = false;
    let mut chars = text.trim_start_matches('\u{feff}').chars().peekable();
    while let Some(c) = chars.next() {
        match (c, quoted) {
            ('"', true) if chars.peek() == Some(&'"') => {
                field.push('"');
                chars.next();
            }
            ('"', _) => quoted = !quoted,
            (',', false) => row.push(std::mem::take(&mut field)),
            ('\r', false) => {}
            ('\n', false) => {
                row.push(std::mem::take(&mut field));
                rows.push(std::mem::take(&mut row));
            }
            _ => field.push(c),
        }
    }
    if !field.is_empty() || !row.is_empty() {
        row.push(field);
        rows.push(row);
    }
    rows
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn imports_beeftext_shape() {
        let mut lib = Library::default();
        let json = r#"{"groups":[{"name":"Work","combos":[{"keyword":"sig","snippet":"Hi","matchingMode":"Loose"}]}]}"#;
        assert_eq!(import_json(json, &mut lib).unwrap(), 1);
        assert_eq!(lib.snippets[0].trigger, "sig");
        assert_eq!(lib.snippets[0].boundary, Boundary::Loose);
        assert_eq!(lib.groups[0].name, "Work");
    }

    #[test]
    fn imports_csv_with_quotes() {
        let mut lib = Library::default();
        let csv = "trigger,content,name\n;a,\"line1\nline2, \"\"x\"\"\",Alpha\n";
        assert_eq!(import_csv(csv, &mut lib).unwrap(), 1);
        assert_eq!(lib.snippets[0].content, "line1\nline2, \"x\"");
    }
}
