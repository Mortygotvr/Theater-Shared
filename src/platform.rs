//! The chat platforms Theater Reader reads.

use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Platform {
    Twitch,
    Kick,
    #[serde(rename = "youtube")]
    YouTube,
}

impl Platform {
    pub const ALL: [Platform; 3] = [Platform::Twitch, Platform::Kick, Platform::YouTube];

    /// As triggers say it
    pub fn name(self) -> &'static str {
        match self {
            Platform::Twitch => "Twitch",
            Platform::Kick => "Kick",
            Platform::YouTube => "YouTube",
        }
    }

    /// Theater Reader's parser (an alert's `source`)
    pub fn parser_name(self) -> &'static str {
        match self {
            Platform::Twitch => "TwitchParser",
            Platform::Kick => "KickParser",
            Platform::YouTube => "YouTubeParser",
        }
    }

    /// Its events, each of which a chat can turn off (Theater Reader's event
    /// filters, same names)
    pub fn events(self) -> &'static [&'static str] {
        match self {
            Platform::Twitch => &[
                "Twitch chat",
                "Twitch cheer",
                "Twitch redeem (irc)",
                "Twitch redeem (pubsub)",
                "Twitch sub",
                "Twitch raid",
                "Twitch hype train",
                "Twitch ban",
                "Twitch timeout",
                "Twitch other",
            ],
            Platform::Kick => &[
                "Kick chat",
                "Kick redeem",
                "Kick follow",
                "Kick sub",
                "Kick gift sub",
                "Kick raid start",
                "Kick raid end",
                "Kick ban",
                "Kick timeout",
                "Kick stream start",
                "Kick stream end",
                "Kick other",
            ],
            Platform::YouTube => &["chat_message", "paid_message", "sticker_message", "member_message"],
        }
    }

    /// What its chats are given as
    pub fn input_hint(self) -> &'static str {
        match self {
            Platform::Twitch => "channel name, e.g. mortygotvr",
            Platform::Kick => "channel name, e.g. mortygotvr",
            Platform::YouTube => "@handle, channel or live video link",
        }
    }

    /// The platform called `name` (any case)
    pub fn named(name: &str) -> Option<Platform> {
        Platform::ALL.into_iter().find(|p| p.name().eq_ignore_ascii_case(name))
    }
}
