//! The relay every Theater plugin sends through: a websocket on port 41837
//! (Theater Reader's), where Theater Scene, overlays and other apps listen.
//!
//! Several Theater plugins may be installed, or just one, in any order, so
//! the first to start takes a role and the others follow it:
//! - **Host**: no other Theater plugin in OBS hosts, and nothing outside OBS
//!   listens on the port: it serves the port, and registers on OBS's global
//!   handlers the procedures `theater_relay_host(out string name, out int
//!   port)` and `theater_relay_send(in string json)`, the signal
//!   `theater_event(string json)` (each message sent, for the Control Panel
//!   inside OBS) and the signal `theater_relay_app(string json)` (what a
//!   connected app sent: Theater Reader's tester…).
//! - **In OBS**: another Theater plugin hosts: messages go through its
//!   `theater_relay_send`, and what apps send comes from its
//!   `theater_relay_app`.
//! - **Outside**: something outside OBS already serves the port (the Theater
//!   Reader app): this plugin connects to it as an app, and emits
//!   `theater_event` itself.
//! - **Guest** (the Theater API plugins, `start_guest`): never serves the
//!   port, which is a Reader's (one at a time: the Theater Reader plugin or
//!   app). Each message goes through the Theater plugin hosting in OBS when
//!   there is one, else only to `theater_event`, where Theater Scene and the
//!   Control Panel hear it inside OBS.
//!
//! Every message an app sends reaches the other apps.

use std::ffi::c_void;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::Duration;

use futures_util::{SinkExt, StreamExt};
use parking_lot::Mutex;
use tokio::sync::mpsc::UnboundedSender;
use tokio_tungstenite::tungstenite::Message;

use crate::obs_link::{self, Calldata};

/// Theater Reader's port, where apps look for the relay
pub const DEFAULT_PORT: u16 = 41837;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Role {
    /// Not started (or not in OBS)
    None,
    Host,
    /// Another Theater plugin hosts (its name)
    InObs(String),
    /// Something outside OBS serves the port
    Outside,
    /// Never serves the port: through the host in OBS, else theater_event
    Guest,
}

#[derive(Clone, Debug)]
pub struct Status {
    pub role: Role,
    pub port: u16,
    /// Serving (host) or connected (outside)
    pub up: bool,
    /// Apps connected (when hosting)
    pub apps: usize,
    /// Why it isn't up
    pub problem: Option<String>,
}

struct State {
    role: Role,
    port: u16,
    name: &'static str,
    log: fn(&str),
    on_app: Option<fn(&str)>,
    /// Outside: the connection's queue
    outside: Option<UnboundedSender<String>>,
    problem: Option<String>,
}

static STATE: Mutex<State> = Mutex::new(State {
    role: Role::None,
    port: DEFAULT_PORT,
    name: "",
    log: |_| {},
    on_app: None,
    outside: None,
    problem: None,
});
static CLIENTS: Mutex<Vec<(u64, UnboundedSender<Message>)>> = Mutex::new(Vec::new());
static NEXT_ID: AtomicU64 = AtomicU64::new(1);
static UP: AtomicBool = AtomicBool::new(false);

fn log(msg: &str) {
    let log = STATE.lock().log;
    log(msg);
}

pub fn status() -> Status {
    let s = STATE.lock();
    Status { role: s.role.clone(), port: s.port, up: UP.load(Ordering::SeqCst), apps: CLIENTS.lock().len(), problem: s.problem.clone() }
}

/// Takes this plugin's role (call once, from obs_module_load: plugins load
/// one after the other, so two can't both become the host).
/// - `name`: the plugin's, for the others' windows ("Theater Reader")
/// - `port`, `lan`: where to serve when hosting (this computer only, or the network too)
/// - `on_app`: what a connected app sent, in any role
pub fn start(name: &'static str, port: u16, lan: bool, runtime: &tokio::runtime::Handle, log: fn(&str), on_app: Option<fn(&str)>) -> Role {
    {
        let mut s = STATE.lock();
        if s.role != Role::None {
            return s.role.clone();
        }
        s.name = name;
        s.log = log;
        s.on_app = on_app;
        s.port = port;
    }
    let Some(obs) = obs_link::obs() else { return Role::None };

    // Another Theater plugin hosts?
    let mut cd = Calldata::new();
    if obs.call(c"theater_relay_host", &mut cd) {
        let host = obs.get_string(&cd, c"name");
        obs.free(&mut cd);
        obs.connect(c"theater_relay_app", from_app_signal);
        let role = Role::InObs(if host.is_empty() { "another Theater plugin".into() } else { host });
        STATE.lock().role = role.clone();
        UP.store(true, Ordering::SeqCst);
        log(&format!("[Relay] {} hosts the relay: messages go through it", role_name(&role)));
        return role;
    }
    obs.free(&mut cd);

    // Something outside OBS on the port?
    let outside = std::net::TcpStream::connect_timeout(&std::net::SocketAddr::from(([127, 0, 0, 1], port)), Duration::from_millis(300)).is_ok();
    obs.add_signal(c"void theater_event(string json)");
    if outside {
        let (tx, rx) = tokio::sync::mpsc::unbounded_channel::<String>();
        let mut s = STATE.lock();
        s.role = Role::Outside;
        s.outside = Some(tx);
        drop(s);
        runtime.spawn(connect_outside(port, rx));
        log(&format!("[Relay] Something outside OBS serves port {port}: joining it as an app"));
        return Role::Outside;
    }

    // Host
    obs.add_signal(c"void theater_relay_app(string json)");
    obs.add_proc(c"void theater_relay_host(out string name, out int port)", host_proc);
    obs.add_proc(c"void theater_relay_send(in string json)", send_proc);
    obs.connect(c"theater_relay_app", from_app_signal);
    STATE.lock().role = Role::Host;
    runtime.spawn(serve(port, lan));
    Role::Host
}

/// Takes the guest's role, for a plugin that sends but never serves the port
/// (call once, from obs_module_load). It works whichever plugin loads first:
/// each message looks for a host in OBS when it's sent.
/// - `on_app`: what an app connected to the host sent
pub fn start_guest(name: &'static str, log: fn(&str), on_app: Option<fn(&str)>) -> Role {
    {
        let mut s = STATE.lock();
        if s.role != Role::None {
            return s.role.clone();
        }
        s.name = name;
        s.log = log;
        s.on_app = on_app;
    }
    let Some(obs) = obs_link::obs() else { return Role::None };
    // Declared here too, so they exist whether or not a host loads after
    obs.add_signal(c"void theater_event(string json)");
    obs.add_signal(c"void theater_relay_app(string json)");
    obs.connect(c"theater_relay_app", from_app_signal);
    STATE.lock().role = Role::Guest;
    UP.store(true, Ordering::SeqCst);
    log("[Relay] Sending through Theater Reader when its plugin hosts the relay, else inside OBS (theater_event)");
    Role::Guest
}

/// The Theater plugin hosting the relay in OBS, if one does
pub fn host_in_obs() -> Option<String> {
    let obs = obs_link::obs()?;
    let mut cd = Calldata::new();
    let found = obs.call(c"theater_relay_host", &mut cd);
    let name = obs.get_string(&cd, c"name");
    obs.free(&mut cd);
    found.then(|| if name.is_empty() { "another Theater plugin".into() } else { name })
}

pub fn role_name(role: &Role) -> String {
    match role {
        Role::None => "Nobody".into(),
        Role::Host => STATE.lock().name.to_string(),
        Role::InObs(name) => name.clone(),
        Role::Outside => "An app outside OBS".into(),
        Role::Guest => host_in_obs().unwrap_or_else(|| "OBS itself (Theater Scene, the Control Panel)".into()),
    }
}

/// To every app listening, and the Control Panel (theater_event). Any thread.
pub fn send(text: &str) {
    let role = STATE.lock().role.clone();
    match role {
        Role::Host => host_send(text),
        Role::InObs(_) => {
            if let Some(obs) = obs_link::obs() {
                let mut cd = Calldata::new();
                obs.set_string(&mut cd, c"json", text);
                obs.call(c"theater_relay_send", &mut cd);
                obs.free(&mut cd);
            }
        }
        Role::Outside => {
            if let Some(tx) = STATE.lock().outside.as_ref() {
                let _ = tx.send(text.to_string());
            }
            if let Some(obs) = obs_link::obs() {
                obs.signal_string(c"theater_event", c"json", text);
            }
        }
        Role::Guest => {
            if let Some(obs) = obs_link::obs() {
                let mut cd = Calldata::new();
                obs.set_string(&mut cd, c"json", text);
                let through_host = obs.call(c"theater_relay_send", &mut cd);
                obs.free(&mut cd);
                if !through_host {
                    obs.signal_string(c"theater_event", c"json", text);
                }
            }
        }
        Role::None => {}
    }
}

fn host_send(text: &str) {
    broadcast(text, None);
    if let Some(obs) = obs_link::obs() {
        obs.signal_string(c"theater_event", c"json", text);
    }
}

fn broadcast(text: &str, except: Option<u64>) {
    let message = Message::text(text.to_string());
    CLIENTS.lock().retain(|(id, tx)| Some(*id) == except || tx.send(message.clone()).is_ok());
}

unsafe extern "C" fn host_proc(_data: *mut c_void, cd: *mut Calldata) {
    let Some(obs) = obs_link::obs() else { return };
    let (name, port) = {
        let s = STATE.lock();
        (s.name, s.port)
    };
    obs.set_string(cd, c"name", name);
    obs.set_int(cd, c"port", port as i64);
}

unsafe extern "C" fn send_proc(_data: *mut c_void, cd: *mut Calldata) {
    let Some(obs) = obs_link::obs() else { return };
    let text = obs.get_string(cd, c"json");
    if !text.is_empty() {
        let _ = std::panic::catch_unwind(|| host_send(&text));
    }
}

unsafe extern "C" fn from_app_signal(_data: *mut c_void, cd: *mut Calldata) {
    let Some(obs) = obs_link::obs() else { return };
    let text = obs.get_string(cd, c"json");
    let on_app = STATE.lock().on_app;
    if let (Some(on_app), false) = (on_app, text.is_empty()) {
        let _ = std::panic::catch_unwind(|| on_app(&text));
    }
}

/// What a connected app sent: to the other apps, and every Theater plugin
fn from_app(id: u64, text: &str) {
    broadcast(text, Some(id));
    if let Some(obs) = obs_link::obs() {
        obs.signal_string(c"theater_relay_app", c"json", text);
    }
}

async fn serve(port: u16, lan: bool) {
    let host = if lan { "0.0.0.0" } else { "127.0.0.1" };
    let listener = loop {
        match tokio::net::TcpListener::bind((host, port)).await {
            Ok(l) => break l,
            Err(e) => {
                let why = if e.kind() == std::io::ErrorKind::AddrInUse {
                    format!("port {port} is taken (is the Theater Reader app running? close it)")
                } else {
                    format!("port {port}: {e}")
                };
                let new_problem = {
                    let mut s = STATE.lock();
                    let new = s.problem.as_deref() != Some(why.as_str());
                    s.problem = Some(why.clone());
                    new
                };
                if new_problem {
                    log(&format!("[Relay] ⚠️ The relay can't start: {why}; trying again every 10 s"));
                }
                tokio::time::sleep(Duration::from_secs(10)).await;
            }
        }
    };
    STATE.lock().problem = None;
    UP.store(true, Ordering::SeqCst);
    log(&format!("[Relay] 📡 Relay on ws://{host}:{port}"));
    loop {
        let Ok((stream, _)) = listener.accept().await else { continue };
        tokio::spawn(client(stream));
    }
}

async fn client(stream: tokio::net::TcpStream) {
    let Ok(ws) = tokio_tungstenite::accept_async(stream).await else { return };
    let id = NEXT_ID.fetch_add(1, Ordering::SeqCst);
    let (mut write, mut read) = ws.split();
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<Message>();
    CLIENTS.lock().push((id, tx));
    let writing = async {
        while let Some(message) = rx.recv().await {
            if write.send(message).await.is_err() {
                break;
            }
        }
    };
    let reading = async {
        while let Some(Ok(message)) = read.next().await {
            match message {
                Message::Text(text) => from_app(id, text.as_str()),
                Message::Close(_) => break,
                _ => {}
            }
        }
    };
    tokio::select! {
        _ = writing => {}
        _ = reading => {}
    }
    CLIENTS.lock().retain(|(i, _)| *i != id);
}

/// Outside: an app of the relay outside OBS, reconnecting when it drops
async fn connect_outside(port: u16, mut rx: tokio::sync::mpsc::UnboundedReceiver<String>) {
    let url = format!("ws://127.0.0.1:{port}");
    loop {
        match tokio_tungstenite::connect_async(url.as_str()).await {
            Ok((ws, _)) => {
                UP.store(true, Ordering::SeqCst);
                STATE.lock().problem = None;
                let (mut write, mut read) = ws.split();
                loop {
                    tokio::select! {
                        out = rx.recv() => match out {
                            Some(text) => if write.send(Message::text(text)).await.is_err() { break },
                            None => return,
                        },
                        incoming = read.next() => match incoming {
                            Some(Ok(Message::Text(text))) => {
                                let on_app = STATE.lock().on_app;
                                if let Some(on_app) = on_app {
                                    on_app(text.as_str());
                                }
                            }
                            Some(Ok(_)) => {}
                            _ => break,
                        },
                    }
                }
                UP.store(false, Ordering::SeqCst);
            }
            Err(e) => {
                STATE.lock().problem = Some(format!("can't reach the relay on port {port}: {e}"));
            }
        }
        tokio::time::sleep(Duration::from_secs(5)).await;
    }
}
