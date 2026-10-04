//! What Theater Scene and Theater Reader (repos Theater-Scene and Theater-Reader) share: the chat
//! platforms, Theater Reader's trigger patterns, the channels and redeem
//! titles alerts came from, and the lists that build a trigger.

pub mod builder;
pub mod platform;
pub mod seen;
pub mod triggers;

pub use platform::Platform;
