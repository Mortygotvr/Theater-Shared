//! The channels and redeem titles alerts came from (Theater Reader's
//! RedemptionTracker idea): the trigger builder's lists. Each app keeps its
//! own, in the file it sets.

use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

use parking_lot::Mutex;
use serde_json::Value;

use crate::Platform;

/// Platform → channel → redeem titles
pub type Seen = BTreeMap<String, BTreeMap<String, BTreeSet<String>>>;

static SEEN: Mutex<Option<Seen>> = Mutex::new(None);
static FILE: Mutex<Option<PathBuf>> = Mutex::new(None);

/// Where the lists are kept (until set: in memory only). Read again from it.
pub fn set_file(path: PathBuf) {
    *FILE.lock() = Some(path);
    *SEEN.lock() = None;
}

fn with<R>(f: impl FnOnce(&mut Seen) -> R) -> R {
    let mut seen = SEEN.lock();
    let seen = seen.get_or_insert_with(|| {
        FILE.lock()
            .as_ref()
            .and_then(|file| std::fs::read_to_string(file).ok())
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default()
    });
    f(seen)
}

fn save(seen: &Seen) {
    let Some(file) = FILE.lock().clone() else { return };
    if let Ok(s) = serde_json::to_string_pretty(seen) {
        let _ = std::fs::write(file, s);
    }
}

/// Everything seen so far.
pub fn all() -> Seen {
    with(|s| s.clone())
}

/// Channels of a platform (None: every platform's).
pub fn channels(platform: Option<Platform>) -> Vec<String> {
    with(|s| {
        let mut out: BTreeSet<String> = BTreeSet::new();
        for (p, chans) in s.iter() {
            if platform.is_none_or(|want| want.name() == p) {
                out.extend(chans.keys().cloned());
            }
        }
        out.into_iter().collect()
    })
}

/// Redeem titles of a platform and channel (None / "": any).
pub fn titles(platform: Option<Platform>, channel: &str) -> Vec<String> {
    with(|s| {
        let mut out: BTreeSet<String> = BTreeSet::new();
        for (p, chans) in s.iter() {
            if platform.is_some_and(|want| want.name() != p) {
                continue;
            }
            for (c, titles) in chans {
                if channel.is_empty() || c.eq_ignore_ascii_case(channel) {
                    out.extend(titles.iter().cloned());
                }
            }
        }
        out.into_iter().collect()
    })
}

/// Adds a channel and some of its titles (saved when it's new).
pub fn add(platform: Platform, channel: &str, titles: &[String]) {
    let channel = channel.trim();
    if channel.is_empty() || matches!(channel, "ST" | "System" | "testserver") {
        return;
    }
    with(|s| {
        let chans = s.entry(platform.name().to_string()).or_default();
        // One entry per channel, whatever its case
        let key = chans.keys().find(|k| k.eq_ignore_ascii_case(channel)).cloned().unwrap_or_else(|| channel.to_string());
        let mut changed = !chans.contains_key(&key);
        let set = chans.entry(key).or_default();
        for t in titles {
            let t = t.trim();
            if !t.is_empty() && t != "Unknown" {
                changed |= set.insert(t.to_string());
            }
        }
        if changed {
            save(s);
        }
    });
}

/// Notes an alert's channel and title (the trigger feed's messages are the
/// same event again; tests aren't noted).
pub fn note(message: &Value) {
    if message.get("type").and_then(|t| t.as_str()) != Some("alert") {
        return;
    }
    let Some(trigger) = message.get("trigger").and_then(|t| t.as_str()) else { return };
    let event = message.get("event").and_then(|e| e.as_str()).unwrap_or("");
    let Some(parts) = crate::triggers::from_event(trigger) else { return };
    let Some(platform) = parts.platform else { return };
    // Twitch's chat says a redeem's id only (the points connection says its title)
    let title = if event.ends_with("(irc)") { String::new() } else { parts.detail };
    add(platform, &parts.channel, &[title]);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn alerts_fill_the_lists_and_are_kept_in_the_file() {
        let file = std::env::temp_dir().join(format!("theater_seen_test_{}.json", std::process::id()));
        let _ = std::fs::remove_file(&file);
        set_file(file.clone());
        note(&serde_json::json!({"type": "alert", "event": "Twitch redeem (pubsub)", "trigger": "Twitch mortygotvr redeem Hydrate"}));
        // Chat, tests and the trigger feed aren't noted
        note(&serde_json::json!({"type": "chat", "trigger": "Twitch someone chat"}));
        note(&serde_json::json!({"type": "alert", "event": "Twitch sub", "trigger": "Twitch testserver sub"}));
        assert_eq!(channels(Some(Platform::Twitch)), vec!["mortygotvr".to_string()]);
        assert_eq!(titles(Some(Platform::Twitch), "MortyGotVR"), vec!["Hydrate".to_string()]);

        // Read back from the file
        set_file(file.clone());
        assert_eq!(titles(None, ""), vec!["Hydrate".to_string()]);
        let _ = std::fs::remove_file(&file);
    }
}
