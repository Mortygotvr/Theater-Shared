//! The trigger lists (triggers), drawn with egui: the Studio's redeem editor
//! and poses, the Chat & Alerts window's tester. What they need from the app
//! they're in (its chats, its messages) comes from its Host.

use std::sync::{Arc, OnceLock};

use crate::Platform;
use crate::triggers::{Detail, Parts, events, from_event, parse};

/// What the lists need from the app they're in.
#[derive(Clone, Copy)]
pub struct Host {
    /// Channels the reader reads, offered first
    pub chats: fn(Option<Platform>) -> Vec<String>,
    /// Every relay message from now on (🎯 Catch next waits for an alert)
    pub subscribe: fn() -> crossbeam_channel::Receiver<Arc<str>>,
}

static HOST: OnceLock<Host> = OnceLock::new();

/// Sets the app's Host, once (until then: no chats, no 🎯 Catch next).
pub fn set_host(host: Host) {
    let _ = HOST.set(host);
}

/// A builder's picks, kept in egui's memory between frames.
#[derive(Clone, Default)]
struct BuilderState {
    parts: Parts,
    /// The trigger being changed (its place in the list)
    editing: Option<usize>,
    /// Typing the triggers as text
    text_mode: bool,
    text: String,
    /// 🎯 Catch next: waiting for an alert
    catching: Option<crossbeam_channel::Receiver<Arc<str>>>,
    /// A channel typed (not in the list)
    typing_channel: bool,
    status: String,
}

/// Channels to offer: the reader's chats, then the ones alerts came from
pub fn channels(platform: Option<Platform>) -> Vec<String> {
    let mut out: Vec<String> = HOST.get().map(|h| (h.chats)(platform)).unwrap_or_default();
    out.retain(|c| !c.trim().is_empty());
    for c in crate::seen::channels(platform) {
        if !out.iter().any(|o| o.eq_ignore_ascii_case(&c)) {
            out.push(c);
        }
    }
    out
}

/// The next alert's trigger, from what `rx` got so far
fn caught(rx: &crossbeam_channel::Receiver<Arc<str>>) -> Option<String> {
    for message in rx.try_iter() {
        let Ok(v) = serde_json::from_str::<serde_json::Value>(&message) else { continue };
        if v.get("type").and_then(|t| t.as_str()) == Some("alert") {
            if let Some(t) = v.get("trigger").and_then(|t| t.as_str()) {
                return Some(t.to_string());
            }
        }
    }
    None
}

/// The four lists and what they make. Returns the pattern once it's whole.
fn lists_ui(ui: &mut egui::Ui, id: egui::Id, st: &mut BuilderState) -> String {
    ui.horizontal_wrapped(|ui| {
        ui.label("Platform");
        let before = st.parts.platform;
        egui::ComboBox::from_id_salt(id.with("platform"))
            .selected_text(st.parts.platform.map(|p| p.name()).unwrap_or("Any"))
            .width(90.0)
            .show_ui(ui, |ui| {
                ui.selectable_value(&mut st.parts.platform, None, "Any");
                for p in Platform::ALL {
                    ui.selectable_value(&mut st.parts.platform, Some(p), p.name());
                }
            });
        if st.parts.platform != before && st.parts.kind().is_none() {
            st.parts.event.clear();
        }

        ui.label("Channel");
        if st.typing_channel {
            ui.add(egui::TextEdit::singleline(&mut st.parts.channel).hint_text("channel").desired_width(110.0));
            if ui.small_button("▾").on_hover_text("Pick from the list").clicked() {
                st.typing_channel = false;
            }
        } else {
            let label = if st.parts.channel.is_empty() { "Any".to_string() } else { st.parts.channel.clone() };
            egui::ComboBox::from_id_salt(id.with("channel")).selected_text(label).width(110.0).show_ui(ui, |ui| {
                ui.selectable_value(&mut st.parts.channel, String::new(), "Any");
                for c in channels(st.parts.platform) {
                    ui.selectable_value(&mut st.parts.channel, c.clone(), c);
                }
                if ui.selectable_label(false, "✏ Type one…").clicked() {
                    st.typing_channel = true;
                }
            });
        }

        ui.label("Event");
        let label = st.parts.kind().map(|k| k.label).unwrap_or("Choose…");
        egui::ComboBox::from_id_salt(id.with("event")).selected_text(label).width(170.0).show_ui(ui, |ui| {
            for k in events(st.parts.platform) {
                ui.selectable_value(&mut st.parts.event, k.words.to_string(), k.label);
            }
        });

        match st.parts.kind().map(|k| k.detail) {
            Some(Detail::Title) => {
                ui.label("Title");
                ui.add(egui::TextEdit::singleline(&mut st.parts.detail).hint_text("any").desired_width(130.0));
                let titles = crate::seen::titles(st.parts.platform, &st.parts.channel);
                if !titles.is_empty() {
                    egui::ComboBox::from_id_salt(id.with("title")).selected_text("").width(24.0).show_ui(ui, |ui| {
                        ui.selectable_value(&mut st.parts.detail, String::new(), "Any");
                        for t in titles {
                            ui.selectable_value(&mut st.parts.detail, t.clone(), t);
                        }
                    });
                }
            }
            Some(Detail::Amount(_)) => {
                ui.weak("any amount").on_hover_text("Amounts never limit a trigger: what fires gets it as {amount}");
            }
            _ => {}
        }
    });
    st.parts.compose()
}

/// A list of triggers, as chips (✕ removes one, a click changes it), and the
/// lists to add one. `max` 1 for a single trigger. Returns true when the list
/// changed.
pub fn triggers_ui(ui: &mut egui::Ui, id_salt: impl std::hash::Hash, triggers: &mut Vec<String>, max: usize) -> bool {
    let id = ui.make_persistent_id(id_salt);
    let mut st: BuilderState = ui.data_mut(|d| d.get_temp::<BuilderState>(id)).unwrap_or_default();
    let mut changed = false;

    if st.text_mode {
        ui.horizontal(|ui| {
            let edit = ui.add(
                egui::TextEdit::singleline(&mut st.text)
                    .hint_text("Reader events, e.g. Twitch * redeem Throw Tomato, Twitch * redeem *")
                    .desired_width((ui.available_width() - 90.0).max(120.0)),
            );
            if edit.changed() {
                *triggers = st.text.split(',').map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect();
                triggers.truncate(max);
                changed = true;
            }
            if ui.button("🧩 Lists").on_hover_text("Build them from lists").clicked() {
                st.text_mode = false;
            }
        });
        ui.data_mut(|d| d.insert_temp(id, st));
        return changed;
    }

    // The chips
    let mut remove = None;
    ui.horizontal_wrapped(|ui| {
        if triggers.is_empty() {
            ui.weak("No trigger yet: it only fires from its button.");
        }
        for (i, t) in triggers.iter().enumerate() {
            let editing = st.editing == Some(i);
            let fill = if editing { ui.visuals().selection.bg_fill } else { ui.visuals().widgets.inactive.bg_fill };
            egui::Frame::new().fill(fill).corner_radius(10.0).inner_margin(egui::Margin::symmetric(8, 2)).show(ui, |ui| {
                ui.horizontal(|ui| {
                    let chip = ui.add(egui::Label::new(t.as_str()).sense(egui::Sense::click()));
                    if chip.on_hover_text("Change it").clicked() {
                        match parse(t) {
                            Some(parts) => {
                                st.typing_channel = !parts.channel.is_empty() && !channels(parts.platform).contains(&parts.channel);
                                st.parts = parts;
                                st.editing = Some(i);
                                st.status.clear();
                            }
                            None => {
                                st.text_mode = true;
                                st.text = triggers.join(", ");
                            }
                        }
                    }
                    if ui.small_button("✕").on_hover_text("Remove it").clicked() {
                        remove = Some(i);
                    }
                });
            });
        }
    });
    if let Some(i) = remove {
        triggers.remove(i);
        st.editing = None;
        changed = true;
    }

    // The lists
    let pattern = lists_ui(ui, id, &mut st);
    ui.horizontal_wrapped(|ui| {
        if !pattern.is_empty() {
            ui.monospace(&pattern);
        }
        match st.editing {
            Some(i) if i < triggers.len() => {
                if ui.add_enabled(!pattern.is_empty(), egui::Button::new("✔ Update")).clicked() {
                    triggers[i] = pattern.clone();
                    st.editing = None;
                    changed = true;
                }
                if ui.button("Cancel").clicked() {
                    st.editing = None;
                }
            }
            _ => {
                let add = if max == 1 && !triggers.is_empty() { "⟳ Replace" } else { "➕ Add" };
                let room = triggers.len() < max || max == 1;
                let fresh = !triggers.iter().any(|t| t.eq_ignore_ascii_case(&pattern));
                if ui.add_enabled(!pattern.is_empty() && room && fresh, egui::Button::new(add)).clicked() {
                    if max == 1 {
                        triggers.clear();
                    }
                    triggers.push(pattern.clone());
                    changed = true;
                }
            }
        }
        if st.catching.is_some() {
            if ui.button("⏹ Stop").clicked() {
                st.catching = None;
                st.status.clear();
            }
        } else if let Some(host) = HOST.get() {
            if ui.button("🎯 Catch next").on_hover_text("Fill the lists from the next alert (a redeem, a cheer…)").clicked() {
                st.catching = Some((host.subscribe)());
                st.status = "Waiting for the next alert… (do the redeem, or test it)".into();
            }
        }
        if ui.button("✏ Edit as text").clicked() {
            st.text_mode = true;
            st.text = triggers.join(", ");
        }
    });
    if let Some(rx) = st.catching.clone() {
        if let Some(trigger) = caught(&rx) {
            match from_event(&trigger) {
                Some(parts) => {
                    st.typing_channel = !parts.channel.is_empty() && !channels(parts.platform).contains(&parts.channel);
                    st.parts = parts;
                    st.status = format!("Caught: {}", trigger);
                }
                None => st.status = format!("Caught \"{}\": the lists don't have it", trigger),
            }
            st.catching = None;
        } else {
            ui.ctx().request_repaint_after(std::time::Duration::from_millis(200));
        }
    }
    if !st.status.is_empty() {
        ui.weak(&st.status);
    }
    ui.data_mut(|d| d.insert_temp(id, st));
    changed
}

/// Just the lists (no chips): the pattern they make, "" until whole.
pub fn pattern_ui(ui: &mut egui::Ui, id_salt: impl std::hash::Hash) -> String {
    let id = ui.make_persistent_id(id_salt);
    let mut st: BuilderState = ui.data_mut(|d| d.get_temp::<BuilderState>(id)).unwrap_or_default();
    let pattern = lists_ui(ui, id, &mut st);
    ui.data_mut(|d| d.insert_temp(id, st));
    pattern
}
