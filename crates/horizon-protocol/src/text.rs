//! Chat components as sent on the network (ComponentSerialization in NBT form)
//! flattened to display strings, with `translate` keys resolved through a
//! language table (en_us.json) and Java `String.format` style arguments.
use horizon_world::nbt::{JavaString, Tag};
use std::collections::HashMap;

/// A language table: translation key -> pattern.
pub type Language = HashMap<String, String>;

/// Flatten a JSON component, used by legacy chat and login disconnects.
pub fn json_plain(value: &serde_json::Value, lang: &Language) -> String {
    fn append(value: &serde_json::Value, lang: &Language, depth: usize) -> String {
        if depth > 32 {
            return String::new();
        }
        match value {
            serde_json::Value::String(s) => s.clone(),
            serde_json::Value::Array(a) => a.iter().map(|v| append(v, lang, depth + 1)).collect(),
            serde_json::Value::Object(o) => {
                let mut out = if let Some(text) = o.get("text").and_then(|v| v.as_str()) {
                    text.to_owned()
                } else if let Some(key) = o.get("translate").and_then(|v| v.as_str()) {
                    let args: Vec<String> = o
                        .get("with")
                        .and_then(|v| v.as_array())
                        .map(|a| a.iter().map(|v| append(v, lang, depth + 1)).collect())
                        .unwrap_or_default();
                    let pattern = lang
                        .get(key)
                        .map(String::as_str)
                        .or_else(|| o.get("fallback").and_then(|v| v.as_str()))
                        .unwrap_or(key);
                    format_java(pattern, &args)
                } else {
                    String::new()
                };
                if let Some(extra) = o.get("extra").and_then(|v| v.as_array()) {
                    for child in extra {
                        out.push_str(&append(child, lang, depth + 1));
                    }
                }
                out
            }
            other => other.to_string(),
        }
    }
    append(value, lang, 0)
}

/// Disconnect screens currently display plain text. Include HTTP links whose
/// labels hide their destination, so legitimate server validation instructions
/// remain usable. This never opens a browser or executes a server command.
pub fn disconnect_json(value: &serde_json::Value, lang: &Language) -> String {
    fn links(value: &serde_json::Value, urls: &mut Vec<String>, depth: usize) {
        if depth > 32 || urls.len() >= 32 {
            return;
        }
        if let Some(a) = value.as_array() {
            for child in a {
                links(child, urls, depth + 1);
            }
        }
        if let Some(o) = value.as_object() {
            let event = o.get("clickEvent").or_else(|| o.get("click_event"));
            if let Some(event) = event
                && event.get("action").and_then(|v| v.as_str()) == Some("open_url")
                && let Some(url) = event
                    .get("value")
                    .or_else(|| event.get("url"))
                    .and_then(|v| v.as_str())
                && (url.starts_with("https://") || url.starts_with("http://"))
                && !url.chars().any(char::is_control)
                && !urls.iter().any(|previous| previous == url)
            {
                urls.push(url.to_owned());
            }
            for key in ["extra", "with"] {
                if let Some(child) = o.get(key) {
                    links(child, urls, depth + 1);
                }
            }
        }
    }
    let mut out = json_plain(value, lang);
    let mut urls = Vec::new();
    links(value, &mut urls, 0);
    for url in urls {
        let destination = url
            .strip_prefix("https://")
            .or_else(|| url.strip_prefix("http://"))
            .unwrap();
        if !out.contains(destination) {
            out.push('\n');
            out.push_str(&url);
        }
    }
    out
}

fn string(tag: &Tag) -> Option<String> {
    match tag {
        Tag::String(s) => Some(s.to_string_lossy()),
        _ => None,
    }
}
fn get<'t>(tag: &'t Tag, key: &str) -> Option<&'t Tag> {
    match tag {
        Tag::Compound(map) => map.get(&JavaString::from(key)),
        _ => None,
    }
}

/// Flatten a component to text.
pub fn plain(tag: &Tag, lang: &Language) -> String {
    let mut out = String::new();
    append(tag, lang, &mut out, 0);
    out
}
fn append(tag: &Tag, lang: &Language, out: &mut String, depth: usize) {
    if depth > 32 {
        return;
    }
    match tag {
        Tag::String(s) => out.push_str(&s.to_string_lossy()),
        Tag::Byte(v) => out.push_str(&v.to_string()),
        Tag::Short(v) => out.push_str(&v.to_string()),
        Tag::Int(v) => out.push_str(&v.to_string()),
        Tag::Long(v) => out.push_str(&v.to_string()),
        Tag::Float(v) => out.push_str(&f32::from_bits(*v).to_string()),
        Tag::Double(v) => out.push_str(&f64::from_bits(*v).to_string()),
        // A list is its first element with the rest appended as siblings.
        Tag::List(items) => {
            for item in items {
                append(item, lang, out, depth + 1);
            }
        }
        Tag::Compound(_) => {
            if let Some(text) = get(tag, "text").and_then(string) {
                out.push_str(&text);
            } else if let Some(value) = get(tag, "") {
                // NbtOps' wrapper for a bare value in a mixed list (a
                // translation's number and string arguments).
                append(value, lang, out, depth + 1);
            } else if let Some(key) = get(tag, "translate").and_then(string) {
                let args: Vec<String> = match get(tag, "with") {
                    Some(Tag::List(args)) => args
                        .iter()
                        .map(|a| {
                            let mut s = String::new();
                            append(a, lang, &mut s, depth + 1);
                            s
                        })
                        .collect(),
                    _ => Vec::new(),
                };
                let pattern = lang
                    .get(&key)
                    .cloned()
                    .or_else(|| get(tag, "fallback").and_then(string))
                    .unwrap_or_else(|| key.clone());
                out.push_str(&format_java(&pattern, &args));
            } else if let Some(key) = get(tag, "keybind").and_then(string) {
                out.push_str(lang.get(&key).map_or(key.as_str(), String::as_str));
            } else if let Some(selector) = get(tag, "selector").and_then(string) {
                out.push_str(&selector);
            } else if let Some(score) = get(tag, "score") {
                if let Some(name) = get(score, "name").and_then(string) {
                    out.push_str(&name);
                }
            }
            if let Some(Tag::List(extra)) = get(tag, "extra") {
                for e in extra {
                    append(e, lang, out, depth + 1);
                }
            }
        }
        _ => {}
    }
}

/// TranslatableContents.decomposeTemplate: `%s`, `%n$s` and `%%`.
pub fn format_java(pattern: &str, args: &[String]) -> String {
    let mut out = String::new();
    let mut next = 0usize;
    let mut chars = pattern.char_indices().peekable();
    while let Some((_, c)) = chars.next() {
        if c != '%' {
            out.push(c);
            continue;
        }
        let mut digits = String::new();
        while let Some(&(_, d)) = chars.peek() {
            if d.is_ascii_digit() {
                digits.push(d);
                chars.next();
            } else {
                break;
            }
        }
        match chars.next() {
            Some((_, '%')) if digits.is_empty() => out.push('%'),
            Some((_, '$')) if !digits.is_empty() => {
                if chars.next().map(|(_, c)| c) == Some('s') {
                    let i = digits.parse::<usize>().unwrap_or(1).saturating_sub(1);
                    out.push_str(args.get(i).map_or("", String::as_str));
                }
            }
            Some((_, 's')) if digits.is_empty() => {
                out.push_str(args.get(next).map_or("", String::as_str));
                next += 1;
            }
            Some((_, other)) => {
                out.push('%');
                out.push_str(&digits);
                out.push(other);
            }
            None => out.push('%'),
        }
    }
    out
}

/// A literal text component.
pub fn literal(text: &str) -> Tag {
    Tag::String(JavaString::from(text))
}
/// A translatable component with literal arguments.
pub fn translatable(key: &str, args: &[&str]) -> Tag {
    let mut entries = vec![("translate", Tag::String(JavaString::from(key)))];
    if !args.is_empty() {
        entries.push(("with", Tag::List(args.iter().map(|a| literal(a)).collect())));
    }
    Tag::compound(entries)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn disconnect_shows_server_instructions_and_hidden_validation_link() {
        let component = serde_json::json!({"text":"", "extra":[
            {"bold":true,"color":"aqua","text":"MUSH\n\n"},
            {"text":"Olá!\nPor segurança precisamos validar sua conexão.\n"},
            {"text":"Clique aqui e digite seu nick.","clickEvent":{
                "action":"open_url","value":"https://mush.com.br/validar"}},
            {"text":"","clickEvent":{"action":"run_command","value":"/login secret"}}
        ]});
        assert_eq!(
            disconnect_json(&component, &Language::new()),
            "MUSH\n\nOlá!\nPor segurança precisamos validar sua conexão.\nClique aqui e digite seu nick.\nhttps://mush.com.br/validar"
        );
    }

    #[test]
    fn disconnect_resolves_translation_without_duplicating_visible_urls() {
        let lang = Language::from([("disconnect.test".into(), "Valide %s".into())]);
        let component = serde_json::json!({"translate":"disconnect.test","with":[
            {"text":"mush.com.br/validar","clickEvent":{
                "action":"open_url","value":"https://mush.com.br/validar"}}
        ]});
        assert_eq!(
            disconnect_json(&component, &lang),
            "Valide mush.com.br/validar"
        );
        assert_eq!(
            json_plain(
                &serde_json::json!(["A",{"text":"B","extra":[{"text":"C"}]}]),
                &lang
            ),
            "ABC"
        );
    }

    #[test]
    fn formats_like_translatable_contents() {
        let args = vec!["Steve".to_owned(), "hi".to_owned()];
        assert_eq!(format_java("<%s> %s", &args), "<Steve> hi");
        assert_eq!(format_java("%2$s by %1$s", &args), "hi by Steve");
        assert_eq!(format_java("100%% sure", &[]), "100% sure");
    }

    #[test]
    fn unwraps_mixed_list_arguments() {
        let mut lang = Language::new();
        lang.insert(
            "commands.item.entity.set.success.single".into(),
            "Replaced %s slot(s) on %s with %s".into(),
        );
        // `with: [1, {text: "Alex"}, ...]` in NBT: the int comes wrapped.
        let replaced = Tag::compound([
            (
                "translate",
                Tag::String("commands.item.entity.set.success.single".into()),
            ),
            (
                "with",
                Tag::List(vec![
                    Tag::compound([("", Tag::Int(1))]),
                    Tag::compound([("text", Tag::String("Alex".into()))]),
                    Tag::compound([("", Tag::String("[Emerald]".into()))]),
                ]),
            ),
        ]);
        assert_eq!(
            plain(&replaced, &lang),
            "Replaced 1 slot(s) on Alex with [Emerald]"
        );
    }

    #[test]
    fn flattens_components() {
        let mut lang = Language::new();
        lang.insert(
            "multiplayer.player.joined".into(),
            "%s joined the game".into(),
        );
        let joined = Tag::compound([
            ("translate", Tag::String("multiplayer.player.joined".into())),
            (
                "with",
                Tag::List(vec![Tag::compound([("text", Tag::String("Alex".into()))])]),
            ),
            ("color", Tag::String("yellow".into())),
        ]);
        assert_eq!(plain(&joined, &lang), "Alex joined the game");
        let extra = Tag::compound([
            ("text", Tag::String("a".into())),
            (
                "extra",
                Tag::List(vec![
                    literal("b"),
                    Tag::compound([("text", Tag::String("c".into()))]),
                ]),
            ),
        ]);
        assert_eq!(plain(&extra, &lang), "abc");
        assert_eq!(
            plain(&translatable("missing.key", &[]), &lang),
            "missing.key"
        );
        assert_eq!(plain(&literal("plain"), &lang), "plain");
    }
}
