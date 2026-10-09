//! Burrow extensions.
//!
//! An extension is a separate program that speaks BXP (the Burrow Extension
//! Protocol, [`protocol`]): JSON-RPC 2.0, one message per line over its
//! stdin/stdout. Its `extension.toml` ([`manifest`]) declares what it
//! contributes (palette commands, read-only sources, libraries of
//! documents) and the capabilities it asks for ([`capability`]); the user
//! grants those in the config file ([`grants`]). The [`host`] runs the
//! enabled extensions and answers their calls from the app's `Backend`
//! ([`api`]); [`ext`] is the other side, for extensions written in Rust.
//!
//! What an extension can never do through Burrow: publish, withdraw, pin,
//! delete, fork, change the blyg's settings, or see a token, the Keychain,
//! the data directory or the database. Files and network access aren't
//! Burrow's to give or withhold (an extension runs as the user), so `fs:`
//! and `net` are declarations shown at consent, not enforcement.

pub mod capability;
pub mod ext;
pub mod grants;
pub mod manifest;
pub mod protocol;
pub mod rpc;

pub use capability::{CONSENT_CAVEAT, Capability, DECLARED_SUFFIX, consent_sentence, describe};
pub use grants::{Grants, settings_from_lines};
pub use manifest::{Diagnostic, Installed, MANIFEST_FILE, Manifest, Origin, discover};
pub use protocol::PROTOCOL_VERSION;
