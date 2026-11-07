//! A small IRC client for Burrow's experimental chat tab (`#thicket` on
//! Libera.Chat).
//!
//! - [`message`]: parse and build IRC lines (tags, prefix, params; CTCP,
//!   formatting codes, rfc1459 case folding, mentions).
//! - [`session`]: one connection's protocol state with no I/O: CAP LS 302,
//!   SASL PLAIN, the welcome, joins, NAMES, PING, and [`Event`]s.
//! - [`room`]: a channel as the UI shows it: members and a bounded log.
//! - [`conn`]: a blocking socket on its own thread (rustls TLS), with
//!   reconnects and backoff. No async runtime, like the rest of Burrow.
//!
//! Written for Burrow (MIT) rather than vendored: Halloy, the obvious
//! off-the-shelf client, is GPL-3.0, and the async IRC crates bring tokio.
//! Nothing here logs or prints, so a password can never reach a log.

pub mod conn;
pub mod message;
pub mod room;
pub mod session;
#[cfg(feature = "test-server")]
pub mod testserver;

pub use conn::{Client, Endpoint, Timing, Tls};
pub use message::{Message, fold, mentions, valid_nick};
pub use room::{Line, LineKind, Room, is_op};
pub use session::{Auth, ChatMessage, Command, Event, Member, MsgKind, SessionConfig};

/// The channel Burrow's chat tab joins.
pub const THICKET: &str = "#thicket";
/// Libera.Chat over TLS.
pub const LIBERA: &str = "irc.libera.chat:6697";
/// Libera.Chat's NICKLEN.
pub const NICK_MAX: usize = 16;
