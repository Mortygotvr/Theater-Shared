//! Every alert in one shape, whatever sent it (Theater Reader, a Theater API
//! plugin…), so Rhai scripts and the Control Panel read `ev.alert.user`,
//! `ev.alert.amount`… instead of each platform's own field names.
//!
//! Senders put it in their relay message as `"alert"`, beside the platform's
//! own `data` (kept as it is, and as `alert.raw`). Theater Scene fills it in
//! for messages that come without one (`from_relay`).
//!
//! ```text
//! kind      redeem, cheer, sub, resub, gift, gift_upgrade, raid, follow,
//!           superchat, supersticker, member, hype_train, timeout, ban,
//!           stream_start, stream_end, donation, pledge, other
//! platform  twitch, kick, youtube… ("" when unknown)
//! channel   the channel it came from ("" when unknown)
//! user      who did it ("" for nobody in particular: a hype train, the stream)
//! anonymous an anonymous gift
//! message   what they wrote with it
//! image     their picture, or the redeem's
//! amount    bits, viewers, gifted subs, the hype train's level, money (as a number)
//! currency  for money: "USD", "$"… as the platform gave it, else ""
//! title     a redeem's title
//! reward_id a redeem's id on its platform
//! cost      a redeem's points
//! tier      "Tier 1", "Tier 2", "Tier 3", "Prime" or ""
//! months    a resub's months (0 when unknown)
//! duration  a timeout's seconds
//! progress, goal   a hype train's
//! test      sent by a tester, not the platform
//! trigger   the Theater trigger ("Twitch mortygotvr redeem Hydrate")
//! raw       the platform's own data
//! ```

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Alert {
    pub kind: String,
    pub platform: String,
    pub channel: String,
    pub user: String,
    pub anonymous: bool,
    pub message: String,
    pub image: String,
    pub amount: f64,
    pub currency: String,
    pub title: String,
    pub reward_id: String,
    pub cost: f64,
    pub tier: String,
    pub months: f64,
    pub duration: f64,
    pub progress: f64,
    pub goal: f64,
    pub test: bool,
    pub trigger: String,
    pub raw: Value,
}

impl Alert {
    pub fn to_value(&self) -> Value {
        serde_json::to_value(self).unwrap_or(Value::Null)
    }
}

fn text(v: Option<&Value>) -> String {
    match v {
        Some(Value::String(s)) => s.trim().to_string(),
        Some(Value::Number(n)) => n.to_string(),
        _ => String::new(),
    }
}

/// The first of `keys` with some text
fn first(data: &Map<String, Value>, keys: &[&str]) -> String {
    keys.iter().map(|k| text(data.get(*k))).find(|t| !t.is_empty()).unwrap_or_default()
}

/// A number from a number or a text ("500", "1,500", "$10.00" → 10)
fn number(v: Option<&Value>) -> f64 {
    match v {
        Some(Value::Number(n)) => n.as_f64().unwrap_or(0.0),
        Some(Value::String(s)) => {
            let digits: String = s.chars().filter(|c| c.is_ascii_digit() || *c == '.' || *c == '-').collect();
            digits.parse().unwrap_or(0.0)
        }
        _ => 0.0,
    }
}

fn first_number(data: &Map<String, Value>, keys: &[&str]) -> f64 {
    keys.iter().map(|k| number(data.get(*k))).find(|n| *n != 0.0).unwrap_or(0.0)
}

/// "$10.00" → "$", "10.00 EUR" → "EUR", "10" → ""
fn currency_of(amount: &str) -> String {
    amount.chars().filter(|c| !c.is_ascii_digit() && !matches!(c, '.' | ',' | ' ' | '-')).collect::<String>().trim().to_string()
}

fn tier_of(plan: &str) -> String {
    match plan {
        "1000" | "1" => "Tier 1",
        "2000" | "2" => "Tier 2",
        "3000" | "3" => "Tier 3",
        p if p.eq_ignore_ascii_case("prime") => "Prime",
        _ => "",
    }
    .to_string()
}

fn platform_of(text: &str) -> &'static str {
    let t = text.to_lowercase();
    if t.contains("twitch") {
        "twitch"
    } else if t.contains("kick") {
        "kick"
    } else if t.contains("youtube") {
        "youtube"
    } else if t.contains("kofi") || t.contains("ko-fi") {
        "kofi"
    } else if t.contains("patreon") {
        "patreon"
    } else {
        ""
    }
}

/// A trigger's channel, its second word ("Twitch mortygotvr redeem X")
fn channel_of(trigger: &str) -> String {
    let mut words = trigger.split_whitespace();
    match (words.next(), words.next()) {
        (Some(p), Some(c)) if !platform_of(p).is_empty() && c != "System" => c.to_string(),
        _ => String::new(),
    }
}

/// A trigger without its channel: "Twitch mortygotvr redeem Doom" → "Twitch redeem Doom"
fn without_channel(trigger: &str) -> String {
    let words: Vec<&str> = trigger.split_whitespace().collect();
    if words.len() >= 3 && !platform_of(words[0]).is_empty() {
        let mut w = words.clone();
        w.remove(1);
        w.join(" ")
    } else {
        words.join(" ")
    }
}

/// The alert in a relay message (`{"type":"alert", source, event, trigger,
/// data}`): its own `alert` when the sender put one in, else made from its
/// data. None when it isn't an alert, or is one Theater shows no alert for
/// (a platform event it doesn't know, the IRC copy of a redeem, a raid ending).
pub fn from_relay(message: &Value) -> Option<Alert> {
    if message.get("type").and_then(Value::as_str) != Some("alert") {
        return None;
    }
    if let Some(given) = message.get("alert").filter(|a| a.is_object()) {
        return serde_json::from_value(given.clone()).ok();
    }
    let source = text(message.get("source"));
    let mut event = text(message.get("event"));
    let trigger = text(message.get("trigger"));
    let empty = Map::new();
    let data = message.get("data").and_then(Value::as_object).unwrap_or(&empty);
    let test = source == "Test UI" || data.get("is_test").and_then(Value::as_bool).unwrap_or(false);
    // A tester sends its trigger as the event ("Twitch testserver cheer")
    if source == "Test UI" {
        event = without_channel(&event);
    }
    let e = event.to_lowercase();
    if e.ends_with(" other") || e.contains("(irc)") || (e.contains("raid") && e.contains("end")) {
        return None;
    }

    let mut platform = platform_of(&format!("{source} {event}")).to_string();
    let youtube = ["paid_message", "sticker_message", "member_message", "super chat", "super sticker"];
    if youtube.iter().any(|y| e.contains(y)) {
        platform = "youtube".into();
    }
    let kind_type = text(data.get("type"));
    let mut a = Alert {
        kind: "other".into(),
        platform,
        channel: channel_of(&trigger),
        user: first(data, &["display_name", "username", "user", "author", "gifter_username", "host_username"]).trim_start_matches('@').to_string(),
        message: first(data, &["message", "user_input"]),
        image: first(data, &["image", "avatar"]),
        test,
        trigger: trigger.clone(),
        raw: Value::Object(data.clone()),
        ..Default::default()
    };

    let gift_types = ["subgift", "anonsubgift", "submysterygift", "giftpaidupgrade"];
    if e.contains("redeem") {
        a.kind = "redeem".into();
        a.title = first(data, &["reward_title", "title"]);
        if a.title.is_empty() || a.title == "Unknown" {
            // From the trigger: "Twitch <channel> redeem <title>"
            let t = without_channel(&trigger);
            a.title = t.find("redeem ").map(|at| t[at + 7..].trim().to_string()).unwrap_or_default();
        }
        a.reward_id = first(data, &["reward_id"]);
        a.cost = first_number(data, &["cost"]);
    } else if e.contains("gift") || gift_types.contains(&kind_type.as_str()) {
        a.kind = if kind_type == "giftpaidupgrade" { "gift_upgrade" } else { "gift" }.into();
        a.anonymous = kind_type == "anonsubgift";
        a.amount = match data.get("gifted_usernames") {
            Some(Value::Array(names)) => names.len() as f64,
            _ => first_number(data, &["count", "quantity", "amount"]),
        };
        if a.kind == "gift" && a.amount == 0.0 {
            a.amount = 1.0;
        }
        a.tier = tier_of(&text(data.get("plan")));
    } else if e.contains("sub") {
        a.kind = if kind_type == "resub" { "resub" } else { "sub" }.into();
        a.tier = tier_of(&first(data, &["plan", "tier"]));
        a.months = first_number(data, &["months", "cumulative_months"]);
    } else if e.contains("cheer") {
        a.kind = "cheer".into();
        a.amount = first_number(data, &["bits", "amount"]);
    } else if e.contains("raid") {
        a.kind = "raid".into();
        a.amount = first_number(data, &["viewers", "viewers_count", "viewer_count", "number_viewers", "amount"]);
    } else if e.contains("paid_message") || e.contains("super chat") {
        a.kind = "superchat".into();
        let shown = first(data, &["amount"]);
        a.amount = number(data.get("amount"));
        a.currency = currency_of(&shown);
    } else if e.contains("sticker_message") || e.contains("super sticker") {
        a.kind = "supersticker".into();
        let shown = first(data, &["amount"]);
        a.amount = number(data.get("amount"));
        a.currency = currency_of(&shown);
    } else if e.contains("member") {
        a.kind = "member".into();
    } else if e.contains("follow") {
        a.kind = "follow".into();
    } else if e.contains("hype train") {
        a.kind = "hype_train".into();
        a.user.clear();
        a.amount = first_number(data, &["level"]);
        a.progress = first_number(data, &["progress"]);
        a.goal = first_number(data, &["goal"]);
    } else if e.contains("timeout") {
        a.kind = "timeout".into();
        a.duration = first_number(data, &["duration"]);
        a.message.clear();
    } else if e.contains("ban") {
        a.kind = "ban".into();
        a.message.clear();
    } else if e.contains("stream start") {
        a.kind = "stream_start".into();
        a.user.clear();
    } else if e.contains("stream end") {
        a.kind = "stream_end".into();
        a.user.clear();
    } else if e.contains("donation") || e.contains("tip") {
        a.kind = "donation".into();
        let shown = first(data, &["amount"]);
        a.amount = number(data.get("amount"));
        a.currency = first(data, &["currency"]);
        if a.currency.is_empty() {
            a.currency = currency_of(&shown);
        }
    } else if e.contains("pledge") {
        a.kind = "pledge".into();
        a.amount = number(data.get("amount"));
        a.currency = first(data, &["currency"]);
    }
    Some(a)
}

/// The relay message with its `alert` filled in when it's an alert without
/// one (Theater Scene, for senders from before the shared format)
pub fn with_alert(mut message: Value) -> Value {
    if message.get("alert").is_none() {
        if let Some(alert) = from_relay(&message) {
            if let Some(m) = message.as_object_mut() {
                m.insert("alert".into(), alert.to_value());
            }
        }
    }
    message
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn alert(m: Value) -> Alert {
        from_relay(&m).expect("an alert")
    }

    #[test]
    fn a_twitch_redeem_with_its_title_cost_and_picture() {
        let a = alert(json!({"type":"alert","source":"TwitchParser","event":"Twitch redeem (pubsub)","trigger":"Twitch mortygotvr redeem Hydrate!",
            "data":{"display_name":"ModSquad","reward_title":"Hydrate!","reward_id":"r1","cost":1500,"image":"https://x/4.png","user_input":"cheers"}}));
        assert_eq!((a.kind.as_str(), a.platform.as_str(), a.channel.as_str()), ("redeem", "twitch", "mortygotvr"));
        assert_eq!((a.user.as_str(), a.title.as_str(), a.reward_id.as_str()), ("ModSquad", "Hydrate!", "r1"));
        assert_eq!((a.cost, a.image.as_str(), a.message.as_str()), (1500.0, "https://x/4.png", "cheers"));
        assert_eq!(a.raw["reward_title"], "Hydrate!");
    }

    #[test]
    fn a_redeem_without_a_title_takes_it_from_the_trigger_and_the_irc_copy_is_none() {
        let a = alert(json!({"type":"alert","source":"KickParser","event":"Kick redeem","trigger":"Kick m redeem Big Hug","data":{"username":"x"}}));
        assert_eq!((a.platform.as_str(), a.title.as_str()), ("kick", "Big Hug"));
        assert!(from_relay(&json!({"type":"alert","source":"TwitchParser","event":"Twitch redeem (irc)","trigger":"Twitch m redeem abc...","data":{}})).is_none());
    }

    #[test]
    fn subs_gifts_cheers_and_raids() {
        let resub = alert(json!({"type":"alert","source":"TwitchParser","event":"Twitch sub","data":{"username":"Ann","type":"resub","plan":"1000","message":"6 months!"}}));
        assert_eq!((resub.kind.as_str(), resub.tier.as_str(), resub.message.as_str()), ("resub", "Tier 1", "6 months!"));
        let anon = alert(json!({"type":"alert","source":"TwitchParser","event":"Twitch sub","data":{"username":"x","type":"anonsubgift","plan":"2000"}}));
        assert_eq!((anon.kind.as_str(), anon.anonymous, anon.amount, anon.tier.as_str()), ("gift", true, 1.0, "Tier 2"));
        let kick_gift = alert(json!({"type":"alert","source":"KickParser","event":"Kick gift sub","data":{"gifter_username":"Santa","gifted_usernames":["a","b"]}}));
        assert_eq!((kick_gift.user.as_str(), kick_gift.amount), ("Santa", 2.0));
        let cheer = alert(json!({"type":"alert","source":"TwitchParser","event":"Twitch cheer","data":{"username":"K","bits":"500"}}));
        assert_eq!((cheer.kind.as_str(), cheer.amount), ("cheer", 500.0));
        let kick_raid = alert(json!({"type":"alert","source":"KickParser","event":"Kick raid start","data":{"host_username":"Raider","number_viewers":12}}));
        assert_eq!((kick_raid.kind.as_str(), kick_raid.user.as_str(), kick_raid.amount), ("raid", "Raider", 12.0));
        assert!(from_relay(&json!({"type":"alert","source":"KickParser","event":"Kick raid end","data":{}})).is_none());
    }

    #[test]
    fn youtube_money_hype_trains_and_moderation() {
        let sc = alert(json!({"type":"alert","source":"YouTubeParser","event":"paid_message","data":{"author":"Tube","amount":"$10.00","message":"hi"}}));
        assert_eq!((sc.kind.as_str(), sc.platform.as_str(), sc.amount, sc.currency.as_str()), ("superchat", "youtube", 10.0, "$"));
        let hype = alert(json!({"type":"alert","source":"TwitchParser","event":"Twitch hype train","data":{"level":3,"progress":250,"goal":1000}}));
        assert_eq!((hype.kind.as_str(), hype.amount, hype.progress, hype.goal, hype.user.as_str()), ("hype_train", 3.0, 250.0, 1000.0, ""));
        let to = alert(json!({"type":"alert","source":"TwitchParser","event":"Twitch timeout","data":{"username":"troll","duration":600,"message":"x"}}));
        assert_eq!((to.kind.as_str(), to.duration, to.message.as_str()), ("timeout", 600.0, ""));
    }

    #[test]
    fn a_tester_alert_and_one_that_already_has_its_alert() {
        let t = alert(json!({"type":"alert","source":"Test UI","event":"Twitch testserver cheer 100","trigger":"Twitch testserver cheer 100","data":{"is_test":true,"username":"TestUser","amount":"100"}}));
        assert_eq!((t.kind.as_str(), t.test, t.amount, t.channel.as_str()), ("cheer", true, 100.0, "testserver"));
        let given = json!({"type":"alert","alert":{"kind":"follow","platform":"twitch","user":"Fan"},"data":{"username":"ignored"}});
        assert_eq!(alert(given.clone()).user, "Fan");
        assert_eq!(with_alert(given.clone()), given, "an alert already there is kept");
        assert!(from_relay(&json!({"type":"chat","message":"hi"})).is_none());
        assert_eq!(with_alert(json!({"type":"alert","event":"Twitch follow","data":{"username":"F"}}))["alert"]["kind"], "follow");
    }
}
