//! blyg-core: everything that isn't pixels. API client, local SQLite store,
//! outbox + sync worker, and the `Backend` trait the UI talks to.

#![forbid(unsafe_code)]

pub mod access_ai; // --- access-ai ---
pub mod agentic; // --- agentic feed --- (studio 0.69, contract 6.2.0)
pub mod api;
pub mod backend;
pub mod background; // --- background --- (studio 0.61, contract 5.10.0)
pub mod browse; // --- browse ---
pub mod catchup; // --- catch up ---
pub mod change_size; // --- background --- isNearSame (studio 0.68)
pub mod changelog; // --- changelog display --- (studio 0.58, contract 5.8.0)
pub mod config;
pub mod contacts; // --- contacts --- (studio 0.56)
pub mod dictionary; // --- dictionary --- (studio 0.69, contract 6.2.0)
pub mod discover; // --- discover --- (studio 0.60, desktop only)
pub mod floor;
pub mod grammar;
pub mod helpers; // --- helpers --- catch-up summary (studio 0.69)
pub mod ids; // SubId, RemoteId: a reading post's two ids, typed apart
pub mod lineage; // --- lineage counts ---
pub mod live;
pub mod loose; // --- loose ends ---
pub mod model;
pub mod nearby; // --- nearby ---
pub mod opml; // --- OPML import/export ---
pub mod order; // the one list order (reading rows, your posts)
pub mod origin; // --- unicode --- §12.2 origin spelling (decision #103)
pub mod outbound; // --- contacts --- sent mentions, groups, sources
pub mod own; // your published posts as reading rows
pub mod pace; // the pace-layered Feed (studio 0.68)
pub mod pick;
pub mod profile; // --- profiles ---
pub mod scratch_media;
pub mod shelf; // --- shelf sync --- (studio 0.61, `GET/PUT /api/shelf`)
pub mod site; // --- site ---
pub mod state;
pub mod store;
pub mod sync;
pub mod syndication; // --- syndication --- (decision #98)
/// Synthetic data at scale, for performance tests and benchmarks.
#[doc(hidden)]
pub mod synth;
pub mod text; // --- unicode --- §5 text rules (decision #103)
pub mod tk;
pub mod trails; // --- trails ---
pub mod upkeep; // --- upkeep --- stale quotes, signals
mod util;
pub mod wait; // the one sleep (crates' clippy.toml)

pub use backend::{
    Backend, CoreError, MediaRef, Promote, Promoted, PublicItem, PublishOutcome, Result,
    STALE_MESSAGE, promotion_kind, stale,
};
pub use config::{Config, ConfigStore};
pub use live::LiveBackend;
pub use model::*;
pub use pick::{PickQuery, PickSort, PickSource, Pickable};
pub use profile::{BlogrollEntry, Profile, ProfileKind, ProfilePost, Relation}; // --- profiles ---
pub use sync::{SyncCadence, SyncOptions};
pub use trails::{Trail, TrailStop}; // --- trails ---
