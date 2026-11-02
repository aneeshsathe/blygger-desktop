//! blyg-core: everything that isn't pixels. API client, local SQLite store,
//! outbox + sync worker, and the `Backend` trait the UI talks to.

pub mod api;
pub mod backend;
pub mod browse; // --- browse ---
pub mod config;
pub mod floor;
pub mod grammar;
pub mod lineage; // --- lineage counts ---
pub mod live;
pub mod model;
pub mod opml; // --- OPML import/export ---
pub mod pick;
pub mod profile; // --- profiles ---
pub mod scratch_media;
pub mod state;
pub mod store;
pub mod sync;
/// Synthetic data at scale, for performance tests and benchmarks.
#[doc(hidden)]
pub mod synth;
pub mod tk;
mod util;

pub use backend::{
    Backend, CoreError, MediaRef, Promote, Promoted, PublicItem, PublishOutcome, Result,
    STALE_MESSAGE, promotion_kind, stale,
};
pub use config::{Config, ConfigStore};
pub use live::LiveBackend;
pub use model::*;
pub use pick::{PickQuery, PickSort, PickSource, Pickable};
pub use profile::{BlogrollEntry, Profile, ProfileKind, ProfilePost, Relation}; // --- profiles ---
pub use sync::SyncOptions;
