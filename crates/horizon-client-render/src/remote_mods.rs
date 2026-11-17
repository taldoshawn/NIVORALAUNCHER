//! Bounded, declarative settings for already compiled PvP modules. Never executable code.
use serde_json::Value;
use std::{
    io::Read,
    path::{Path, PathBuf},
    sync::{
        Mutex, OnceLock,
        atomic::{AtomicU64, Ordering},
    },
    time::{SystemTime, UNIX_EPOCH},
};
const LIMIT: u64 = 16384;
static STATE: Mutex<Option<Value>> = Mutex::new(None);
static LAST: AtomicU64 = AtomicU64::new(0);
static CACHE: OnceLock<PathBuf> = OnceLock::new();

pub fn validate(value: &Value) -> bool {
    if value["revision"]
        .as_u64()
        .is_none_or(|r| r > u32::MAX as u64)
    {
        return false;
    }
    let Some(mods) = value["mods"].as_array().filter(|m| m.len() <= 5) else {
        return false;
    };
    let mut ids = std::collections::BTreeSet::new();
    for m in mods {
        let Some(id) = m["id"].as_str() else {
            return false;
        };
        if super::pvp_feature(id).is_none() || !ids.insert(id) {
            return false;
        }
        for (key, max) in [("title", 80), ("description", 400)] {
            if let Some(v) = m.get(key) {
                if v.as_str()
                    .is_none_or(|s| s.len() > max || s.chars().any(char::is_control))
                {
                    return false;
                }
            }
        }
        if m.get("available").is_some_and(|v| !v.is_boolean()) {
            return false;
        }
        if let Some(defaults) = m.get("defaults") {
            let Some(fields) = defaults.as_object() else {
                return false;
            };
            for (key, v) in fields {
                let Some(s) = v.as_str() else {
                    return false;
                };
                if key == "corner" {
                    if !["top_left", "top_right", "bottom_left", "bottom_right"].contains(&s) {
                        return false;
                    }
                } else if ![
                    "show_1pct_low",
                    "backdrop",
                    "show_mouse",
                    "show_cps",
                    "show_dir",
                    "show_clock",
                    "show_pct",
                    "show_held",
                ]
                .contains(&key.as_str())
                    || !["true", "false"].contains(&s)
                {
                    return false;
                }
            }
        }
    }
    true
}

fn apply(value: Value) -> bool {
    if !validate(&value) {
        return false;
    }
    let Ok(mut state) = STATE.lock() else {
        return false;
    };
    if state
        .as_ref()
        .is_some_and(|old| old["revision"].as_u64() > value["revision"].as_u64())
    {
        return false;
    }
    *state = Some(value);
    true
}

fn entry(id: &str) -> Option<Value> {
    STATE.lock().ok()?.as_ref()?["mods"]
        .as_array()?
        .iter()
        .find(|m| m["id"] == id)
        .cloned()
}
pub fn available(id: &str) -> bool {
    entry(id)
        .and_then(|m| m["available"].as_bool())
        .unwrap_or(true)
}
pub fn labels(id: &str, title: &str, description: &str) -> (String, String) {
    let m = entry(id).unwrap_or(Value::Null);
    (
        m["title"].as_str().unwrap_or(title).into(),
        m["description"].as_str().unwrap_or(description).into(),
    )
}
pub fn default_setting(id: &str, key: &str) -> Option<String> {
    entry(id)?["defaults"][key].as_str().map(str::to_owned)
}

/// Nonblocking refresh, at most once per minute; failures retain the last valid cache.
pub fn refresh(home: &Path) {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let last = LAST.load(Ordering::Relaxed);
    if now.saturating_sub(last) < 60
        || LAST
            .compare_exchange(last, now, Ordering::AcqRel, Ordering::Relaxed)
            .is_err()
    {
        return;
    }
    let path = CACHE
        .get_or_init(|| home.join("nivora-mods-cache.json"))
        .clone();
    let _ = std::thread::Builder::new()
        .name("NivoraMods".into())
        .spawn(move || {
            if STATE.lock().is_ok_and(|s| s.is_none()) {
                if let Ok(f) = std::fs::File::open(&path) {
                    if let Ok(v) = serde_json::from_reader(f.take(LIMIT + 1)) {
                        apply(v);
                    }
                }
            }
            let result = (|| -> Result<Value, Box<dyn std::error::Error>> {
                let response = ureq::AgentBuilder::new()
                    .redirects(0)
                    .build()
                    .get("https://nivora-launcher.vercel.app/api/mods-config")
                    .timeout(std::time::Duration::from_secs(5))
                    .call()?;
                let mut bytes = Vec::new();
                response
                    .into_reader()
                    .take(LIMIT + 1)
                    .read_to_end(&mut bytes)?;
                if bytes.len() > LIMIT as usize {
                    return Err("Manifest too large".into());
                }
                Ok(serde_json::from_slice(&bytes)?)
            })();
            if let Ok(value) = result {
                if apply(value.clone()) {
                    if let Some(parent) = path.parent() {
                        let _ = std::fs::create_dir_all(parent);
                    }
                    let temp = path.with_extension("tmp");
                    if std::fs::write(&temp, value.to_string()).is_ok() {
                        let _ = std::fs::rename(temp, path);
                    }
                }
            }
        });
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rejects_code_unknown_modules_and_bad_defaults() {
        for v in [
            serde_json::json!({"revision":1,"mods":[{"id":"../evil"}]}),
            serde_json::json!({"revision":1,"mods":[{"id":"pvp-fps","defaults":{"script":"run"}}]}),
            serde_json::json!({"revision":1,"mods":[{"id":"pvp-fps","available":"false"}]}),
        ] {
            assert!(!validate(&v));
        }
    }
    #[test]
    fn refresh_changes_defaults_without_changing_local_preferences_and_keeps_last_good() {
        let local: std::collections::BTreeMap<String, String> =
            [("backdrop".into(), "false".into())].into();
        assert!(apply(
            serde_json::json!({"revision":20,"mods":[{"id":"pvp-fps","title":"FPS novo","defaults":{"backdrop":"true"}}]})
        ));
        assert_eq!(
            local
                .get("backdrop")
                .cloned()
                .or_else(|| default_setting("pvp-fps", "backdrop")),
            Some("false".into())
        );
        assert_eq!(labels("pvp-fps", "FPS", "").0, "FPS novo");
        assert!(!apply(serde_json::json!({"revision":19,"mods":[]})));
        assert!(!apply(
            serde_json::json!({"revision":21,"mods":[{"id":"evil"}]})
        ));
        assert_eq!(default_setting("pvp-fps", "backdrop"), Some("true".into()));
    }
}
