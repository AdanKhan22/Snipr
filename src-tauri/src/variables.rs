//! Snippet variable rendering: `#{name}` / `#{name:arg}`, nestable, e.g. `#{upper:#{clipboard}}`.

use chrono::{Duration, Local, Months};
use rand::Rng;
use std::fmt::Write as _;

const CURSOR: char = '\u{E000}';
const KEY_START: char = '\u{E001}';
const KEY_END: char = '\u{E002}';

#[derive(Debug, Clone, PartialEq)]
pub enum Part {
    Text(String),
    Cursor,
    Key(String),
}

/// The user cancelled an input prompt: abort the expansion.
#[derive(Debug)]
pub struct Cancelled;

pub struct Ctx<'a> {
    pub clipboard: String,
    /// Regex capture groups (index 0 = whole match).
    pub captures: Vec<String>,
    pub allow_shell: bool,
    /// Preview mode: no prompts, no shell, no counter side effects.
    pub preview: bool,
    /// Look up another snippet's content by name or trigger.
    pub lookup: &'a dyn Fn(&str) -> Option<String>,
    pub prompt: &'a dyn Fn(&str, &str) -> Option<String>,
    pub counter: &'a dyn Fn(&str) -> i64,
}

pub fn render(content: &str, ctx: &Ctx) -> Result<Vec<Part>, Cancelled> {
    let s = expand(content, ctx, 0)?;
    Ok(split(&s))
}

fn clean(s: &str) -> String {
    s.chars().filter(|c| !matches!(*c, CURSOR | KEY_START | KEY_END)).collect()
}

fn split(s: &str) -> Vec<Part> {
    let mut parts = vec![];
    let mut text = String::new();
    let mut chars = s.chars();
    while let Some(c) = chars.next() {
        match c {
            CURSOR => {
                if !text.is_empty() {
                    parts.push(Part::Text(std::mem::take(&mut text)));
                }
                parts.push(Part::Cursor);
            }
            KEY_START => {
                if !text.is_empty() {
                    parts.push(Part::Text(std::mem::take(&mut text)));
                }
                let key: String = chars.by_ref().take_while(|c| *c != KEY_END).collect();
                parts.push(Part::Key(key));
            }
            _ => text.push(c),
        }
    }
    if !text.is_empty() {
        parts.push(Part::Text(text));
    }
    parts
}

fn expand(s: &str, ctx: &Ctx, depth: u8) -> Result<String, Cancelled> {
    if depth > 6 {
        return Ok(s.to_string());
    }
    let chars: Vec<char> = s.chars().collect();
    let mut out = String::new();
    let mut i = 0;
    while i < chars.len() {
        // `##{` is an escaped literal `#{`
        if chars[i] == '#' && chars.get(i + 1) == Some(&'#') && chars.get(i + 2) == Some(&'{') {
            out.push_str("#{");
            i += 3;
            continue;
        }
        if chars[i] == '#' && chars.get(i + 1) == Some(&'{') {
            let mut d = 1;
            let mut j = i + 2;
            while j < chars.len() && d > 0 {
                match chars[j] {
                    '{' => d += 1,
                    '}' => d -= 1,
                    _ => {}
                }
                j += 1;
            }
            if d == 0 {
                let inner: String = chars[i + 2..j - 1].iter().collect();
                out.push_str(&eval_var(&inner, ctx, depth)?);
                i = j;
                continue;
            }
        }
        out.push(chars[i]);
        i += 1;
    }
    Ok(out)
}

fn eval_var(inner: &str, ctx: &Ctx, depth: u8) -> Result<String, Cancelled> {
    let (name_raw, arg_raw) = match inner.split_once(':') {
        Some((n, a)) => (n, Some(a)),
        None => (inner, None),
    };
    let name = name_raw.trim().to_lowercase();

    // Regex capture groups: #{1}, #{2}, #{match}
    if let Ok(n) = name.parse::<usize>() {
        return Ok(ctx.captures.get(n).cloned().unwrap_or_default());
    }

    let arg = match arg_raw {
        Some(a) => expand(a, ctx, depth + 1)?,
        None => String::new(),
    };

    Ok(match name.as_str() {
        "match" => ctx.captures.first().cloned().unwrap_or_default(),
        "clipboard" => clean(&ctx.clipboard),
        "cursor" => CURSOR.to_string(),
        "date" | "time" | "datetime" => format_date(&name, &arg),
        "input" => {
            let (label, default) = arg.split_once('|').unwrap_or((arg.as_str(), ""));
            if ctx.preview {
                format!("<{}>", label)
            } else {
                match (ctx.prompt)(label, default) {
                    Some(v) => clean(&v),
                    None => return Err(Cancelled),
                }
            }
        }
        "env" | "envvar" => std::env::var(arg.trim()).unwrap_or_default(),
        "upper" => arg.to_uppercase(),
        "lower" => arg.to_lowercase(),
        "title" => title_case(&arg),
        "trim" | "trimmed" => arg.trim().to_string(),
        "snippet" | "combo" => match (ctx.lookup)(arg.trim()) {
            Some(content) => expand(&content, ctx, depth + 1)?,
            None => format!("#{{{}}}", inner),
        },
        "key" => format!("{}{}{}", KEY_START, arg.trim().to_lowercase(), KEY_END),
        "uuid" => crate::model::new_id(),
        "random" => {
            let (lo, hi) = arg
                .split_once('-')
                .and_then(|(a, b)| Some((a.trim().parse::<i64>().ok()?, b.trim().parse::<i64>().ok()?)))
                .unwrap_or((0, 99));
            let (lo, hi) = if lo <= hi { (lo, hi) } else { (hi, lo) };
            rand::thread_rng().gen_range(lo..=hi).to_string()
        }
        "counter" => {
            if ctx.preview {
                "1".into()
            } else {
                (ctx.counter)(arg.trim()).to_string()
            }
        }
        "shell" | "powershell" => {
            if ctx.preview {
                "[shell output]".into()
            } else if !ctx.allow_shell {
                "[shell disabled in settings]".into()
            } else {
                clean(&run_shell(&arg))
            }
        }
        // Unknown variable: leave untouched so typos are visible.
        _ => format!("#{{{}}}", inner),
    })
}

fn title_case(s: &str) -> String {
    let mut out = String::new();
    let mut start = true;
    for c in s.chars() {
        if c.is_alphanumeric() {
            if start {
                out.extend(c.to_uppercase());
            } else {
                out.extend(c.to_lowercase());
            }
            start = false;
        } else {
            start = true;
            out.push(c);
        }
    }
    out
}

fn run_shell(cmd: &str) -> String {
    use std::process::Command;
    #[cfg(windows)]
    let output = {
        use std::os::windows::process::CommandExt;
        Command::new("powershell")
            .args(["-NoProfile", "-NonInteractive", "-Command", cmd])
            .creation_flags(0x0800_0000) // CREATE_NO_WINDOW
            .output()
    };
    #[cfg(not(windows))]
    let output = Command::new("sh").args(["-c", cmd]).output();
    match output {
        Ok(o) => String::from_utf8_lossy(&o.stdout).trim_end().to_string(),
        Err(e) => format!("[shell error: {}]", e),
    }
}

// ---------------------------------------------------------------- dates

fn format_date(kind: &str, arg: &str) -> String {
    let (fmt, offset) = arg.split_once('|').unwrap_or((arg, ""));
    let fmt = fmt.trim();
    let default = match kind {
        "date" => "%Y-%m-%d",
        "time" => "%H:%M",
        _ => "%Y-%m-%d %H:%M",
    };
    let fmt = if fmt.is_empty() {
        default.to_string()
    } else if fmt.contains('%') {
        fmt.to_string()
    } else {
        qt_to_strftime(fmt)
    };
    let dt = apply_offset(Local::now(), offset);
    let mut out = String::new();
    if write!(out, "{}", dt.format(&fmt)).is_err() {
        return "[bad date format]".into();
    }
    out
}

/// Parses offsets like `+1M-2d+3h` (s, m, h, d, w, M = months, y = years).
fn apply_offset(mut dt: chrono::DateTime<Local>, offset: &str) -> chrono::DateTime<Local> {
    let chars: Vec<char> = offset.trim().chars().collect();
    let mut i = 0;
    while i < chars.len() {
        let sign: i64 = match chars[i] {
            '+' => 1,
            '-' => -1,
            _ => {
                i += 1;
                continue;
            }
        };
        i += 1;
        let start = i;
        while i < chars.len() && chars[i].is_ascii_digit() {
            i += 1;
        }
        let n: i64 = chars[start..i].iter().collect::<String>().parse().unwrap_or(0);
        let Some(unit) = chars.get(i).copied() else { break };
        i += 1;
        let n = n * sign;
        dt = match unit {
            's' => dt + Duration::seconds(n),
            'm' => dt + Duration::minutes(n),
            'h' => dt + Duration::hours(n),
            'd' => dt + Duration::days(n),
            'w' => dt + Duration::weeks(n),
            'M' | 'y' => {
                let months = (n.unsigned_abs() as u32).saturating_mul(if unit == 'y' { 12 } else { 1 });
                let r = if n >= 0 { dt.checked_add_months(Months::new(months)) } else { dt.checked_sub_months(Months::new(months)) };
                r.unwrap_or(dt)
            }
            _ => dt,
        };
    }
    dt
}

/// Converts Qt-style date formats (used by Beeftext), e.g. `yyyy-MM-dd HH:mm`, to strftime.
fn qt_to_strftime(f: &str) -> String {
    const TOKENS: &[(&str, &str)] = &[
        ("yyyy", "%Y"), ("yy", "%y"), ("MMMM", "%B"), ("MMM", "%b"), ("MM", "%m"), ("M", "%-m"),
        ("dddd", "%A"), ("ddd", "%a"), ("dd", "%d"), ("d", "%-d"),
        ("HH", "%H"), ("H", "%-H"), ("hh", "%I"), ("h", "%-I"),
        ("mm", "%M"), ("m", "%-M"), ("ss", "%S"), ("s", "%-S"),
        ("zzz", "%3f"), ("AP", "%p"), ("ap", "%P"),
    ];
    let mut out = String::new();
    let mut rest = f;
    'outer: while !rest.is_empty() {
        for (tok, rep) in TOKENS {
            if rest.starts_with(tok) {
                out.push_str(rep);
                rest = &rest[tok.len()..];
                continue 'outer;
            }
        }
        let c = rest.chars().next().unwrap();
        if c == '%' {
            out.push_str("%%");
        } else {
            out.push(c);
        }
        rest = &rest[c.len_utf8()..];
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ctx<'a>(
        lookup: &'a dyn Fn(&str) -> Option<String>,
        prompt: &'a dyn Fn(&str, &str) -> Option<String>,
        counter: &'a dyn Fn(&str) -> i64,
    ) -> Ctx<'a> {
        Ctx { clipboard: "clip".into(), captures: vec!["m".into(), "g1".into()], allow_shell: false, preview: false, lookup, prompt, counter }
    }

    #[test]
    fn renders_nested_and_cursor() {
        let l = |_: &str| None;
        let p = |label: &str, _: &str| Some(format!("<{}>", label));
        let c = |_: &str| 7;
        let ctx = ctx(&l, &p, &c);
        let parts = render("Hi #{upper:#{clipboard}}#{cursor}!", &ctx).unwrap();
        assert_eq!(parts, vec![Part::Text("Hi CLIP".into()), Part::Cursor, Part::Text("!".into())]);
        let parts = render("#{1}-#{counter:x}-##{no}", &ctx).unwrap();
        assert_eq!(parts, vec![Part::Text("g1-7-#{no}".into())]);
    }

    #[test]
    fn keys_and_unknown() {
        let l = |_: &str| None;
        let p = |_: &str, _: &str| None;
        let c = |_: &str| 0;
        let ctx = ctx(&l, &p, &c);
        let parts = render("a#{key:tab}b #{bogus}", &ctx).unwrap();
        assert_eq!(parts, vec![Part::Text("a".into()), Part::Key("tab".into()), Part::Text("b #{bogus}".into())]);
        assert!(render("#{input:Name}", &ctx).is_err());
    }

    #[test]
    fn qt_formats_convert() {
        assert_eq!(qt_to_strftime("yyyy-MM-dd HH:mm"), "%Y-%m-%d %H:%M");
    }
}
