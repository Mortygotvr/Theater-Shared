//! What Theater Scene and Theater Reader (repos Theater-Scene and Theater-Reader) share: alerts in
//! one shape (alert), the chat
//! platforms, Theater Reader's trigger patterns, the channels and redeem
//! titles alerts came from, and the lists that build a trigger.

pub mod alert;
pub mod builder;
pub mod platform;
pub mod seen;
pub mod triggers;

pub use platform::Platform;
