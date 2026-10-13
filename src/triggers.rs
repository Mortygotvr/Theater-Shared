//! Triggers made from lists: Platform · Channel · Event · Detail, the same
//! in the Studio's redeem editor as in the Control Panel (the lists
//! themselves: builder).
//!
//! A trigger is Theater Reader's, "Platform channel event detail", e.g.
//! "Twitch mortygotvr redeem Hydrate"; a pattern has `*` for "any". Amounts
//! (bits, Super Chats, hype train levels) are never part of it: any amount
//! matches, and it's handed on to what fires (`{amount}`).

use crate::Platform;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Detail {
    None,
    /// A redeem's title
    Title,
    /// An amount, then these words (" bits")
    Amount(&'static str),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct EventKind {
    /// As the trigger says it
    pub words: &'static str,
    /// As the list says it
    pub label: &'static str,
    pub detail: Detail,
}

const fn kind(words: &'static str, label: &'static str, detail: Detail) -> EventKind {
    EventKind { words, label, detail }
}

const TWITCH: &[EventKind] = &[
    kind("redeem", "Channel points redeem", Detail::Title),
    kind("cheer", "Cheer (bits)", Detail::Amount(" bits")),
    kind("sub", "Sub / resub / gift", Detail::None),
    kind("raid", "Raid", Detail::None),
    kind("hype train", "Hype train (level)", Detail::Amount("")),
    kind("ban", "Ban", Detail::None),
    kind("timeout", "Timeout", Detail::None),
    // From Theater API · Twitch (Theater Reader doesn't see these)
    kind("follow", "Follow", Detail::None),
    kind("stream start", "Stream started", Detail::None),
    kind("stream end", "Stream ended", Detail::None),
];

const KICK: &[EventKind] = &[
    kind("redeem", "Channel points redeem", Detail::Title),
    kind("follow", "Follow", Detail::None),
    kind("sub", "Sub", Detail::None),
    kind("gift sub", "Gifted subs", Detail::None),
    kind("raid", "Raid (host)", Detail::None),
    kind("raid end", "Raid ended", Detail::None),
    kind("ban", "Ban", Detail::None),
    kind("timeout", "Timeout", Detail::None),
    kind("stream start", "Stream started", Detail::None),
    kind("stream end", "Stream ended", Detail::None),
];

const YOUTUBE: &[EventKind] = &[
    kind("super chat", "Super Chat (amount)", Detail::Amount("")),
    kind("super sticker", "Super Sticker (amount)", Detail::Amount("")),
    kind("member", "New member", Detail::None),
];

/// The events of a platform (None: any platform's, each once)
pub fn events(platform: Option<Platform>) -> Vec<EventKind> {
    match platform {
        Some(Platform::Twitch) => TWITCH.to_vec(),
        Some(Platform::Kick) => KICK.to_vec(),
        Some(Platform::YouTube) => YOUTUBE.to_vec(),
        None => {
            let mut all: Vec<EventKind> = Vec::new();
            for e in TWITCH.iter().chain(KICK).chain(YOUTUBE) {
                if !all.iter().any(|a| a.words == e.words) {
                    all.push(*e);
                }
            }
            all
        }
    }
}

pub fn platform_named(name: &str) -> Option<Platform> {
    Platform::named(name)
}

/// A trigger, in its lists. Empty is "any".
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Parts {
    pub platform: Option<Platform>,
    pub channel: String,
    /// An EventKind's words
    pub event: String,
    /// A redeem's title
    pub detail: String,
}

impl Parts {
    pub fn kind(&self) -> Option<EventKind> {
        events(self.platform).into_iter().find(|e| e.words == self.event)
    }

    /// The pattern ("" until an event is chosen)
    pub fn compose(&self) -> String {
        let Some(kind) = self.kind() else { return String::new() };
        let platform = self.platform.map(|p| p.name()).unwrap_or("*");
        let channel = if self.channel.trim().is_empty() { "*" } else { self.channel.trim() };
        let mut out = format!("{} {} {}", platform, channel, kind.words);
        match kind.detail {
            Detail::None => {}
            Detail::Title => {
                let title = self.detail.trim();
                out.push(' ');
                out.push_str(if title.is_empty() { "*" } else { title });
            }
            Detail::Amount(after) => {
                out.push_str(" *");
                out.push_str(after);
            }
        }
        out
    }
}

fn is_any(word: &str) -> bool {
    word == "*" || (word.starts_with('{') && word.ends_with('}'))
}

/// A pattern's parts, when the lists can say it: None for anything else
/// (it's then edited as text).
pub fn parse(pattern: &str) -> Option<Parts> {
    let words: Vec<&str> = pattern.split_whitespace().collect();
    if words.len() < 3 {
        return None;
    }
    let platform = if is_any(words[0]) { None } else { Some(platform_named(words[0])?) };
    let channel = if is_any(words[1]) { String::new() } else { words[1].to_string() };
    if channel.contains('*') {
        return None;
    }
    let rest = &words[2..];
    // The longest event that starts the rest ("raid end" over "raid")
    let kind = events(platform)
        .into_iter()
        .filter(|k| {
            let kw: Vec<&str> = k.words.split(' ').collect();
            rest.len() >= kw.len() && kw.iter().zip(rest).all(|(a, b)| a.eq_ignore_ascii_case(b))
        })
        .max_by_key(|k| k.words.len())?;
    let detail_words = &rest[kind.words.split(' ').count()..];
    let detail = match kind.detail {
        Detail::None => {
            if !detail_words.is_empty() {
                return None;
            }
            String::new()
        }
        Detail::Title => {
            if detail_words.len() == 1 && is_any(detail_words[0]) {
                String::new()
            } else if detail_words.is_empty() || detail_words.iter().any(|w| w.contains('*') || is_any(w)) {
                return None;
            } else {
                detail_words.join(" ")
            }
        }
        Detail::Amount(after) => {
            // Any amount only: "*" and its words
            let want: Vec<&str> = std::iter::once("*").chain(after.split_whitespace()).collect();
            let fits = detail_words.len() == want.len()
                && detail_words.iter().zip(&want).all(|(a, b)| if *b == "*" { is_any(a) } else { a.eq_ignore_ascii_case(b) });
            if !fits {
                return None;
            }
            String::new()
        }
    };
    Some(Parts { platform, channel, event: kind.words.to_string(), detail })
}

/// An event's trigger (a Theater Reader alert's, e.g. "Twitch mortygotvr
/// cheer 500 bits") in the lists, amount left out: 🎯 Catch next. None
/// for chat and what the lists don't have.
pub fn from_event(trigger: &str) -> Option<Parts> {
    let words: Vec<&str> = trigger.split_whitespace().collect();
    if words.len() < 3 {
        return None;
    }
    let platform = platform_named(words[0])?;
    let rest = &words[2..];
    let kind = events(Some(platform))
        .into_iter()
        .filter(|k| {
            let kw: Vec<&str> = k.words.split(' ').collect();
            rest.len() >= kw.len() && kw.iter().zip(rest).all(|(a, b)| a.eq_ignore_ascii_case(b))
        })
        .max_by_key(|k| k.words.len())?;
    let detail_words = &rest[kind.words.split(' ').count()..];
    let detail = if kind.detail == Detail::Title { detail_words.join(" ") } else { String::new() };
    let channel = match words[1] {
        // The tester's channel and shared chat's: any
        "testserver" | "ST" | "System" => String::new(),
        c => c.to_string(),
    };
    Some(Parts { platform: Some(platform), channel, event: kind.words.to_string(), detail })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parts(platform: Option<Platform>, channel: &str, event: &str, detail: &str) -> Parts {
        Parts { platform, channel: channel.into(), event: event.into(), detail: detail.into() }
    }

    #[test]
    fn lists_make_patterns() {
        assert_eq!(parts(Some(Platform::Twitch), "", "redeem", "Hydrate").compose(), "Twitch * redeem Hydrate");
        assert_eq!(parts(Some(Platform::Twitch), "mortygotvr", "redeem", "").compose(), "Twitch mortygotvr redeem *");
        assert_eq!(parts(Some(Platform::Twitch), "", "cheer", "").compose(), "Twitch * cheer * bits");
        assert_eq!(parts(Some(Platform::Twitch), "", "cheer", "500").compose(), "Twitch * cheer * bits", "never an amount");
        assert_eq!(parts(None, "", "sub", "").compose(), "* * sub");
        assert_eq!(parts(Some(Platform::YouTube), "@me", "super chat", "").compose(), "YouTube @me super chat *");
        assert_eq!(parts(Some(Platform::Kick), "", "raid end", "").compose(), "Kick * raid end");
        assert_eq!(parts(Some(Platform::Kick), "", "", "").compose(), "", "no event yet");
    }

    #[test]
    fn patterns_come_back_to_the_lists() {
        for pattern in [
            "Twitch * redeem Hydrate",
            "Twitch mortygotvr redeem Throw Tomato",
            "Twitch * redeem *",
            "Twitch * cheer * bits",
            "* * sub",
            "Kick * raid end",
            "Kick mortygotvr raid",
            "YouTube * super sticker *",
        ] {
            let p = parse(pattern).unwrap_or_else(|| panic!("{pattern}"));
            assert_eq!(p.compose(), pattern);
        }
        assert_eq!(parse("twitch * REDEEM Hydrate").unwrap().compose(), "Twitch * redeem Hydrate");
        assert_eq!(parse("Twitch * redeem Throw {1}"), None, "a wildcard inside a title: text");
        assert_eq!(parse("Twitch * cheer 500 bits"), None, "an exact amount: text");
        assert_eq!(parse("Twitch * chat"), None, "no chat triggers");
        assert_eq!(parse("Twitch * sub extra"), None);
        assert_eq!(parse("Something else"), None);
        assert_eq!(parse("Twitch {1} cheer {amount} bits").unwrap().compose(), "Twitch * cheer * bits");
    }

    #[test]
    fn an_alert_fills_the_lists_without_its_amount() {
        assert_eq!(from_event("Twitch mortygotvr cheer 500 bits").unwrap(), parts(Some(Platform::Twitch), "mortygotvr", "cheer", ""));
        assert_eq!(from_event("Twitch mortygotvr redeem Throw Tomato").unwrap().detail, "Throw Tomato");
        assert_eq!(from_event("Kick mortygotvr raid").unwrap().event, "raid");
        assert_eq!(from_event("Twitch testserver sub").unwrap().channel, "");
        assert_eq!(from_event("YouTube @me super chat $5.00").unwrap().compose(), "YouTube @me super chat *");
        assert_eq!(from_event("Twitch mortygotvr chat"), None);
    }
}
