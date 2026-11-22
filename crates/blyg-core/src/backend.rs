//! The seam between UI and core. The app only ever talks to `dyn Backend`.
//!
//! Two classes of method:
//! - **Local** (no network, must return in well under a millisecond for a few
//!   thousand items): reads, `create_draft`, `save`, `set_kind`. Mutations
//!   write SQLite, enqueue an outbox op, and return. The sync worker pushes.
//! - **Remote** (blocking network call; the UI runs these on a background
//!   executor): publish, pin, withdraw, media, subscriptions, … They return the
//!   server's verdict because the user is waiting on it.

use crate::model::*;

#[derive(Debug, thiserror::Error)]
pub enum CoreError {
    #[error("offline")]
    Offline,
    #[error("not authorised: sign in to your blyg again")]
    Unauthorized,
    #[error("not found")]
    NotFound,
    /// Server said no; `message` is its `message` field, `details` its
    /// `details` (validation issues, unresolved references).
    #[error("{message}")]
    Rejected {
        status: u16,
        message: String,
        details: Vec<String>,
    },
    /// The blyg doesn't speak the private API contract this Burrow needs
    /// (5.x, blygger-studio 0.49.12 or later, `crate::api::CONTRACT_MAJOR`):
    /// nothing is pushed or pulled. A 404 on a route would otherwise read
    /// as "deleted", and a page in another shape as the end of a list.
    #[error("your blyg needs updating: Burrow needs blygger-studio 0.49.12 or later")]
    ServerOutdated,
    /// 409 `revision-conflict`: the post changed on the blyg after the
    /// revision this command carried was read. Nothing was applied.
    #[error("this post changed on your blyg since Burrow last read it")]
    RevisionConflict,
    /// 409 `cursor-expired`: a list changed on the blyg while it was read
    /// page by page. The pages read so far are thrown away; the list is read
    /// again from the start (never appended to the old pages).
    #[error("a list changed on your blyg while Burrow was reading it")]
    CursorExpired,
    /// --- tangle --- 409 `tangle-confirmation-required` (studio 0.63): the
    /// thread's `![[+ …]]` spans and `[[+ …]]` placeholders would publish
    /// `count` new items, named `parts` (a placeholder's name ends
    /// " (linked)"). Nothing was written. Ask the author, then publish again
    /// with `PublishExtras::tangle` (tangling plan §2.2).
    #[error("publishing this thread also publishes {count} new item{}", if *.count == 1 { "" } else { "s" })]
    TangleConfirm { count: usize, parts: Vec<String> },
    /// Item has never reached the server (still local-only) and the op needs a server id.
    #[error("not on the server yet — still syncing")]
    NotSynced,
    #[error("storage: {0}")]
    Storage(String),
    /// 429: the blyg's API work budget is spent (studio 0.28+). Transient:
    /// nothing is dropped, and the next attempt waits `retry_after` seconds
    /// (the server's `Retry-After`).
    #[error("the blyg asked Burrow to slow down; retrying in {retry_after}s")]
    RateLimited { retry_after: u64 },
    #[error("{0}")]
    Other(String),
}

pub type Result<T> = std::result::Result<T, CoreError>;

/// The message of a [`stale`] refusal.
pub const STALE_MESSAGE: &str = "changed since it was read";

/// [`Backend::save_if_base`]'s refusal: `Rejected{409}` with the working
/// copy's current `content_hash` as the only detail.
pub fn stale(current_hash: String) -> CoreError {
    CoreError::Rejected {
        status: 409,
        message: STALE_MESSAGE.into(),
        details: vec![current_hash],
    }
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct PublishOutcome {
    pub version: u32,
    pub permalink: String,
    pub warning: Option<String>,
    /// --- tangle --- the server ids of the new items this publish minted or
    /// finished, in source order (`parts`, studio 0.63); each was published
    /// before the thread, and is held here now.
    pub parts: Vec<String>,
    /// --- tangle --- the working copy as the server rewrote it (each span
    /// now `![[id | label]]` or `[[id | text]]`), when this publish tangled.
    /// It is the post's text and base here already; an editor that kept its
    /// own text must take this, or it would save the spans back and the next
    /// publish would mint them again.
    pub rewritten: Option<String>,
}

/// Someone's post fetched from its public item document because it isn't
/// held here (see [`Backend::public_item`]).
#[derive(Debug, Clone, PartialEq)]
pub struct PublicItem {
    /// Not subscribed: `subscription_id` is empty and it's never marked read.
    pub item: ReadingItem,
    /// Its public changelog, oldest first (current row with media and lineage).
    pub versions: Vec<RemoteVersion>,
}

/// The largest media file the app reads (the Worker's upload cap is 5 MB).
pub const MEDIA_MAX_BYTES: u64 = 20 * 1024 * 1024;

pub struct MediaRef {
    /// Relative, e.g. "media/abc123.webp" — what goes in the markdown.
    pub url: String,
    pub mime: String,
}

// --- scratch notes ---

/// What `Backend::promote` turns a scratch note into.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Promote {
    /// A server draft (synced, unpublished).
    Draft,
    /// Published right away, with an optional version note.
    Publish { note: Option<String> },
}

/// The result of `Backend::promote`.
pub struct Promoted {
    /// The kind the item was promoted as (see [`promotion_kind`]).
    pub kind: Kind,
    /// Set for `Promote::Publish`.
    pub published: Option<PublishOutcome>,
}

/// The kind a scratch note becomes when it's promoted: a thread when the
/// note is already a thread (the user pressed ⌘T) or its published length
/// is over [`FRAGMENT_LIMIT`] (the server's count, [`published_len`]);
/// otherwise a fragment.
pub fn promotion_kind(kind: Kind, content_md: &str) -> Kind {
    if kind == Kind::Thread || published_len(content_md) > FRAGMENT_LIMIT {
        Kind::Thread
    } else {
        Kind::Fragment
    }
}

pub trait Backend: Send + Sync {
    // ---------- local ----------
    /// Newest-updated first.
    fn items(&self) -> Vec<Item>;
    /// Case-insensitive match over content; same order as `items`. Empty query = `items()`.
    /// --- loose ends --- operators (`is:orphan`, `has:tk`, …,
    /// `crate::loose`) filter what the remaining words find.
    fn search(&self, query: &str) -> Vec<Item>;
    /// --- loose ends --- what `is:orphan` and `quotes:<host>` need beyond
    /// the posts: local reads only (instant, any thread).
    fn loose_facts(&self) -> crate::loose::Facts {
        crate::loose::Facts::from_reading(self.base_url().as_deref(), &self.reading())
    }
    fn item(&self, id: &LocalId) -> Option<Item>;
    // --- syndication --- (decision #98, `crate::syndication`) copies of a
    // post on other networks. Additive; the defaults keep none and refuse
    // changes, so implementors compile.
    /// Your post's copies elsewhere, as last left here. Local, instant.
    fn syndication(&self, id: &LocalId) -> Vec<crate::syndication::SyndicationEntry> {
        let _ = id;
        Vec::new()
    }
    /// Replace your published post's list. Local first and queued (written
    /// to the blyg in place: never a new version). Returns the list as kept
    /// (normalized: https only, one per address).
    fn set_syndication(
        &self,
        id: &LocalId,
        entries: &[crate::syndication::SyndicationEntry],
    ) -> Result<Vec<crate::syndication::SyndicationEntry>> {
        let _ = (id, entries);
        Err(CoreError::Other("copies elsewhere aren't kept here".into()))
    }
    /// Read your published post's list from the blyg again (blocking);
    /// what's kept here after. An edit still waiting to be sent wins.
    fn refresh_syndication(
        &self,
        id: &LocalId,
    ) -> Result<Vec<crate::syndication::SyndicationEntry>> {
        Ok(self.syndication(id))
    }
    /// Whether the blyg keeps copies elsewhere (its capabilities, as last
    /// checked). The list can still be edited here when it doesn't; it
    /// waits on this Mac.
    fn syndication_served(&self) -> bool {
        false
    }
    /// An imported post's copies elsewhere, as its blyg served them. Local;
    /// nothing is fetched from the copies.
    fn reading_syndication(
        &self,
        sub_id: &SubId,
        remote_id: &RemoteId,
    ) -> Vec<crate::syndication::SyndicationEntry> {
        let _ = (sub_id, remote_id);
        Vec::new()
    }
    // --- end syndication ---
    /// --- contacts --- The owner's address book as last fetched (local,
    /// instant). Empty when the blyg doesn't serve contacts.
    fn contacts(&self) -> Vec<crate::contacts::Contact> {
        Vec::new()
    }
    /// --- contacts --- Fetch the address book again (blocking).
    fn refresh_contacts(&self) -> Result<()>;
    /// --- contacts --- Whether publish rewrites `@handle` to a link here
    /// (contract 5.7, as last seen).
    fn rewrites_handles(&self) -> bool {
        false
    }
    /// --- contacts --- `(subscription id, remote id)` of the held posts
    /// that mention you, from the blyg's mentions lens (blocking). `None`
    /// when it isn't served.
    fn reading_mentions(&self) -> Option<Vec<(SubId, RemoteId)>> {
        None
    }
    /// --- contacts --- The lens's last answer (`reading_mentions`), kept
    /// so the Mentions feed's first paint already holds it. Local. `None`:
    /// not served, or never asked.
    fn reading_mentions_cached(&self) -> Option<Vec<(SubId, RemoteId)>> {
        None
    }
    /// --- contacts --- Whether the blyg keeps an address book Burrow can
    /// manage (the Contacts sheet). Contract 5.7 serves both together.
    fn keeps_contacts(&self) -> bool {
        self.rewrites_handles()
    }
    // --- contacts --- managing the book (blocking; the local copy follows
    // each change), the Mentions screen's other reads, and the poll status.
    /// `POST /api/contacts`. `NotFound`: the blyg keeps no contacts.
    fn create_contact(
        &self,
        c: &crate::contacts::ContactInput,
    ) -> Result<crate::contacts::Contact> {
        let _ = c;
        Err(CoreError::NotFound)
    }
    /// `PATCH /api/contacts/{id}`.
    fn update_contact(
        &self,
        id: &str,
        c: &crate::contacts::ContactInput,
    ) -> Result<crate::contacts::Contact> {
        let _ = (id, c);
        Err(CoreError::NotFound)
    }
    /// `DELETE /api/contacts/{id}`.
    fn delete_contact(&self, id: &str) -> Result<()> {
        let _ = id;
        Err(CoreError::NotFound)
    }
    /// `POST /api/contacts/{id}/resolve`: the blyg reads the contact's page
    /// for the identifiers it vouches for.
    fn resolve_contact(&self, id: &str) -> Result<crate::contacts::Contact> {
        let _ = id;
        Err(CoreError::NotFound)
    }
    /// What to offer after subscribing to a blyg: its author's name and
    /// address from `{origin}blyg.json`, else `title` and `origin`
    /// (blocking, bounded). `None` when the book already holds it.
    fn contact_seed(&self, origin: &str, title: &str) -> Option<crate::contacts::ContactSeed> {
        let seed = crate::contacts::ContactSeed::from_author(None, title, origin);
        crate::contacts::held(&self.contacts(), &seed.url, Some(origin))
            .is_none()
            .then_some(seed)
    }
    /// `GET /api/mentions?direction=outbound`: the Webmentions your blyg
    /// sent. `Ok(None)`: not served.
    fn outbound_mentions(&self) -> Result<Option<Vec<crate::outbound::OutboundMention>>> {
        Ok(None)
    }
    /// `GET /api/mentions/summary`. `Ok(None)`: not served.
    fn mention_summary(&self) -> Result<Option<crate::outbound::MentionSummary>> {
        Ok(None)
    }
    /// `GET /api/mention-groups`. `Ok(None)`: not served.
    fn mention_groups(&self) -> Result<Option<Vec<crate::outbound::MentionGroup>>> {
        Ok(None)
    }
    /// `GET /api/mentions/{id}/source`. `Ok(None)`: not served.
    fn mention_source(&self, id: &str) -> Result<Option<crate::outbound::MentionSource>> {
        let _ = id;
        Ok(None)
    }
    /// Read the subscriptions again now (their poll status), blocking.
    fn refresh_subscriptions(&self) -> Result<()>;
    // --- end contacts ---
    /// --- capture --- A web page's `<title>`, fetched once with the
    /// token-less public client (blocking). `None`: not served, not HTML,
    /// or no title.
    fn page_title(&self, url: &str) -> Option<String> {
        let _ = url;
        None
    }
    fn create_draft(&self, kind: Kind, content_md: &str) -> Result<LocalId>;
    /// Save the working copy. Debounced push to `PUT /api/items/:id`.
    fn save(&self, id: &LocalId, content_md: &str) -> Result<()>;
    /// `save` from a keystroke: the same save, but it may land a moment
    /// later, written on another thread, so typing never waits on the
    /// store. Every read through this backend sees it at once (`item`
    /// lays it over the stored row; other calls write it first). A write
    /// that fails is reported as `CoreEvent::Error`. Additive; the
    /// default is `save`.
    fn save_typing(&self, id: &LocalId, content_md: &str) -> Result<()> {
        self.save(id, content_md)
    }
    /// Fragment ⇄ thread. Before first publish this is free (we recreate the
    /// draft server-side if needed); after publish the server decides.
    fn set_kind(&self, id: &LocalId, kind: Kind) -> Result<()>;
    /// Save the working copy together with who generated its TK scopes: one
    /// entry per scope, in order (`None` = written by hand). Call it when
    /// the app has just generated text; plain `save` keeps tracked
    /// provenance attached to its scopes as the text is edited. Pushed with
    /// the combined `PUT …/tk-provenance` (docs/SPEC.md § Client-recorded
    /// provenance). Local; refuses (`Rejected{400}`) an array that doesn't
    /// fit the text. Additive; the default refuses so implementors compile.
    fn save_with_provenance(
        &self,
        id: &LocalId,
        content_md: &str,
        scopes: &[Option<ScopeProvenance>],
    ) -> Result<()> {
        let _ = (id, content_md, scopes);
        Err(CoreError::Other("provenance isn't supported here".into()))
    }
    /// `save`, but only if the working copy is still the text whose
    /// [`content_hash`] is `base_hash` (an extension editing an item it read
    /// earlier). A mismatch refuses with [`stale`] (`Rejected{409}`, the
    /// current hash in `details[0]`) and changes nothing. Local. Additive:
    /// the default compares and then saves (a tiny window between the two);
    /// `LiveBackend` checks and writes under the store's lock.
    fn save_if_base(&self, id: &LocalId, content_md: &str, base_hash: &str) -> Result<()> {
        let item = self.item(id).ok_or(CoreError::NotFound)?;
        let current = content_hash(&item.content_md);
        if current != base_hash {
            return Err(stale(current));
        }
        self.save(id, content_md)
    }
    fn sync_status(&self) -> SyncStatus;
    /// The blyg's origin (`https://blyg.example.com`, no trailing slash), for
    /// resolving relative `media/…` links. `None` when not connected.
    fn base_url(&self) -> Option<String>;
    /// Register the single event sink (called from a background thread).
    fn set_event_sink(&self, sink: Box<dyn Fn(CoreEvent) + Send + Sync>);
    fn resolve_conflict(&self, id: &LocalId, how: Resolution) -> Result<()>;
    /// Last-known reading list from the local cache (instant).
    fn reading(&self) -> Vec<ReadingItem>;
    /// Your own item whose server id is `server_id`, as `items()` has it.
    /// Additive; the default searches `items()`, the live backend looks it
    /// up (the preview's resolver asks for one id at a time).
    fn own_item(&self, server_id: &str) -> Option<Item> {
        self.items()
            .into_iter()
            .find(|i| i.server_id.as_ref().is_some_and(|s| s.0 == server_id))
    }
    /// `reading()`'s rows whose `remote_id` is `remote_id`, in its order.
    /// Additive; the default filters `reading()`, the live backend without
    /// copying the rest.
    fn reading_with_id(&self, remote_id: &RemoteId) -> Vec<ReadingItem> {
        self.reading()
            .into_iter()
            .filter(|r| r.remote_id == *remote_id)
            .collect()
    }
    /// How many of `reading()`'s posts are to read
    /// ([`ReadingItem::to_read`]: never read, or edited since you read
    /// them); Background posts never are (withdrawn ones, or what the
    /// studio's Background lens lists). The live backend counts without
    /// copying the list.
    fn unread_count(&self) -> usize {
        let set = self.background_set();
        self.reading()
            .iter()
            .filter(|r| {
                let background = match &set {
                    Some(s) => s.of(&(r.subscription_id.clone(), r.remote_id.clone())).on,
                    None => r.state == "tombstone",
                };
                r.to_read() && !background
            })
            .count()
    }
    fn subscriptions(&self) -> Vec<Subscription>;
    /// Record that the user has seen the current version of a reading item
    /// (and of any duplicates of the same post). Local only.
    fn mark_read(&self, sub_id: &SubId, remote_id: &RemoteId) -> Result<()>;

    /// Mark many reading items (`(sub_id, remote_id)`, each with its
    /// duplicates) read at their current versions, or unread: a selection
    /// in the reading list, or all of it. Local and instant; on a server
    /// that syncs read state the change is queued for it (unread only when
    /// the server can clear read state; otherwise unread stays on this Mac).
    /// Returns how many rows changed.
    fn set_read(&self, rows: &[(SubId, RemoteId)], read: bool) -> Result<usize>;

    // --- scratch notes ---
    // docs/SPEC.md § Scratch notes. Scratch items have `Status::Scratch`:
    // `save`, `set_kind`, `search` and `delete_draft` work on them locally,
    // and nothing about them is ever enqueued, pushed or pulled.

    /// Create a local-only scratch note. Local (no network, ever). Additive;
    /// the default refuses so implementors compile.
    fn create_scratch(&self, kind: Kind, content_md: &str) -> Result<LocalId> {
        let _ = (kind, content_md);
        Err(CoreError::Other(
            "scratch notes aren't supported here".into(),
        ))
    }

    /// Turn a scratch note into a server item, keeping its `LocalId` (no
    /// duplicate). Its kind becomes [`promotion_kind`] (returned in
    /// `Promoted::kind`).
    /// - `Promote::Draft` is local: the item becomes a `Draft` and a
    ///   `create` is queued in the outbox like any new draft (so it works
    ///   offline and syncs later).
    /// - `Promote::Publish` does the same, then publishes (blocking, like
    ///   `publish`). Offline, the promotion to a draft stands and stays queued;
    ///   the publish itself fails with `Offline`.
    ///
    /// On an item that isn't scratch (already promoted) `Draft` is a no-op
    /// and `Publish` is a plain `publish`. There's no demotion back to scratch.
    fn promote(&self, id: &LocalId, to: Promote) -> Result<Promoted> {
        let _ = (id, to);
        Err(CoreError::Other(
            "scratch notes aren't supported here".into(),
        ))
    }

    // ---------- remote (blocking) ----------
    /// Flush this item's outbox first, then publish.
    fn publish(&self, id: &LocalId, note: Option<&str>) -> Result<PublishOutcome>;
    fn withdraw(&self, id: &LocalId, note: Option<&str>) -> Result<u32>;
    /// Irrevocable. UI must confirm first.
    fn pin(&self, id: &LocalId, version: u32) -> Result<()>;
    fn versions(&self, id: &LocalId) -> Result<Vec<Version>>;
    /// Load version `v` into the working copy (server-side), then refresh local.
    fn restore(&self, id: &LocalId, version: u32) -> Result<()>;
    /// Drafts only (local-only drafts just vanish).
    fn delete_draft(&self, id: &LocalId) -> Result<()>;
    fn upload_media(
        &self,
        bytes: Vec<u8>,
        mime: &str,
        item: Option<&LocalId>,
        alt: Option<&str>,
    ) -> Result<MediaRef>;
    /// Remove an uploaded file by its `media/…` URL (or bare id), e.g. an
    /// upload whose placeholder the user deleted before it finished.
    /// `NotFound` when the server doesn't have it (or can't remove media).
    /// Additive; the default refuses so implementors compile.
    fn delete_media(&self, url: &str) -> Result<()> {
        let _ = url;
        Err(CoreError::Other(
            "removing media isn't supported here".into(),
        ))
    }
    fn fork(&self, of: &RemoteRef) -> Result<LocalId>;
    /// Set whether the item's page shows its verified responses. Returns
    /// whether it shows them now (with `Default`, the blyg's setting decides).
    fn set_responses(&self, id: &LocalId, mode: ResponsesMode) -> Result<bool>;
    /// Set whether the item's public page highlights its generated text
    /// (studio 0.27; remote, needs `owner:publish`). Returns whether it
    /// highlights now (with `Default`, `highlight_generated_default` decides).
    fn set_highlight(&self, id: &LocalId, mode: HighlightMode) -> Result<bool>;

    /// The `[[` / `![[` picker's search over what's held here (local): own
    /// published posts and posts imported from blyg subscriptions, every
    /// word matching. The default searches `items()` and `reading()` in
    /// memory; the live backend uses its full-text indexes.
    fn pick_search(&self, q: &crate::pick::PickQuery) -> Vec<crate::pick::Pickable> {
        crate::pick::search_in(&self.items(), &self.reading(), &self.subscriptions(), q)
    }

    /// Where the picker's query is typed (`picker_typing`), as the blyg's
    /// settings last said. Local; `Auto` until they've been read.
    fn picker_typing(&self) -> PickerTyping {
        PickerTyping::Auto
    }

    /// One of the blyg's own media files (`{base}/media/…`), fetched with
    /// the owner credential, since an upload no public version uses yet is
    /// private (studio 0.28). Remote. The default fetches anonymously.
    fn fetch_own_media(&self, url: &str) -> Result<(Vec<u8>, Option<String>)> {
        crate::api::public::PublicClient::new().get_bytes(url, MEDIA_MAX_BYTES)
    }

    /// Pull everything (items, reading, subscriptions) now.
    fn sync_now(&self) -> Result<()>;
    fn preview_subscription(&self, url: &str) -> Result<SubscribePreview>;
    fn subscribe(&self, url: &str, title: Option<&str>) -> Result<Subscription>;
    fn unsubscribe(&self, sub_id: &SubId) -> Result<()>;
    fn set_subscription(
        &self,
        sub_id: &SubId,
        in_blogroll: Option<bool>,
        title: Option<&str>,
    ) -> Result<()>;
    fn pause_subscription(&self, sub_id: &SubId, paused: bool) -> Result<()>;
    /// Name a subscription: `Some(name)` is a name of your own (the source
    /// stops renaming it); `None` hands the name back to the source, which
    /// refreshes it while polling (studio 0.30, `title_follows_source`).
    /// Additive; the default sets a name and refuses `None`.
    fn rename_subscription(&self, sub_id: &SubId, title: Option<&str>) -> Result<()> {
        match title {
            Some(t) => self.set_subscription(sub_id, None, Some(t)),
            None => Err(CoreError::Other(
                "following the source's name isn't supported here".into(),
            )),
        }
    }
    /// "Check all feeds now": the server polls every subscription that
    /// isn't paused, in the background (studio 0.30), and answers with how
    /// many; a pull follows. Additive; the default refuses.
    fn poll_subscriptions(&self) -> Result<u32> {
        Err(CoreError::Other(
            "checking feeds isn't supported here".into(),
        ))
    }
    /// "Check now" for one subscription: a blyg's index is reconciled at
    /// once (true when anything changed); a feed is polled with the rest.
    /// A pull follows. Additive; the default refuses.
    fn check_subscription(&self, sub_id: &SubId) -> Result<bool> {
        let _ = sub_id;
        Err(CoreError::Other(
            "checking feeds isn't supported here".into(),
        ))
    }
    /// thumb: Some(1) / Some(-1) / None (clear).
    fn signal(&self, sub_id: &SubId, remote_id: &RemoteId, thumb: Option<i8>) -> Result<()>;
    fn mentions(&self) -> Result<Vec<Mention>>;
    fn set_mention_hidden(&self, mention_id: &str, hidden: bool) -> Result<()>;
    fn settings(&self) -> Result<Settings>;
    fn save_settings(&self, settings: &Settings) -> Result<()>;

    // --- discover --- the blygs listed on blygger.com (studio 0.60;
    // `crate::discover`). Desktop only.

    /// The public directory, minus nothing (blocking; read only when the
    /// user opens Discover). Errors carry the words to show.
    fn discover_directory(&self) -> Result<Vec<crate::discover::DirectoryBlyg>> {
        Err(CoreError::Other("Discover isn't offered here".into()))
    }

    /// `Settings.discoverSeenAt` (`""`: never looked). `Ok(None)`: this blyg
    /// doesn't keep it (a studio before 0.60), so nothing is marked new.
    fn discover_seen_at(&self) -> Result<Option<String>> {
        Ok(None)
    }

    /// Move `discoverSeenAt` (a partial `PATCH`, that key only).
    fn set_discover_seen_at(&self, at: &str) -> Result<()> {
        let _ = at;
        Err(CoreError::Other("Discover isn't offered here".into()))
    }
    // --- end discover ---

    // ------- other people's versions & pins (remote, on demand, never polled)
    //
    // These read the public surface of the item's origin, unauthenticated: the
    // owner token never leaves the user's own blyg. Spec §8.4: only the current
    // version and pinned versions have content; the rest of the changelog is
    // metadata the UI shows without any affordance to open it.

    /// The public `changelog` of a reading item, oldest first, from
    /// `{origin}items/{id}.json` (blyg-kind subscriptions; RSS/L0 items return
    /// just their current version). Cached briefly; offline falls back to the
    /// last fetched changelog.
    fn remote_versions(&self, sub_id: &SubId, remote_id: &RemoteId) -> Result<Vec<RemoteVersion>> {
        let _ = (sub_id, remote_id);
        Err(CoreError::Other("unsupported".into()))
    }

    /// What the version browser shows for someone else's post: only the
    /// current and pinned versions (`model::shown_versions` over
    /// `remote_versions`). Unpinned versions don't appear at all.
    fn remote_shown_versions(
        &self,
        sub_id: &SubId,
        remote_id: &RemoteId,
    ) -> Result<Vec<RemoteVersion>> {
        self.remote_versions(sub_id, remote_id)
            .map(|v| shown_versions(&v))
    }

    /// A pinned version, from `{origin}items/{id}/v{n}.json` (cached forever).
    /// Refused locally, with no request for the version, when the changelog
    /// doesn't mark `version` pinned; that refusal and a 404 are both
    /// `Rejected{status: 404}` ("not pinned"). A `content_hash` mismatch is
    /// flagged in `hash_mismatch`, not an error.
    fn remote_pinned(
        &self,
        sub_id: &SubId,
        remote_id: &RemoteId,
        version: u32,
    ) -> Result<PinnedVersion> {
        let _ = (sub_id, remote_id, version);
        Err(CoreError::Other("unsupported".into()))
    }

    /// When the version the user last read (`read_version`) is pinned, that
    /// pinned version, so the UI can diff pinned → current (both sides
    /// public). `None` otherwise (unread, unpinned, not fetchable): the UI then
    /// shows only the changelog notes in between, never a diff.
    fn pinned_diff_base(&self, sub_id: &SubId, remote_id: &RemoteId) -> Option<PinnedVersion> {
        let _ = (sub_id, remote_id);
        None
    }

    // --- AI ---
    //
    // Additive, default-implemented (so other implementors compile): what the
    // app needs to record and disclose text it generated.

    /// The TK provenance tracked locally for an item, as `(keyed_to,
    /// scopes)`: one entry per scope of the text `keyed_to` (carry it to the
    /// current text with `tk::remap`). `None` = nothing tracked here.
    fn tracked_tk_provenance(&self, id: &LocalId)
    -> Option<(String, Vec<Option<ScopeProvenance>>)>;

    /// False when the server can't record the AI provenance of text
    /// generated in the app (it would then publish without disclosure).
    /// Every studio Burrow supports takes it with the text; a backend that
    /// hasn't found out yet says true.
    fn provenance_available(&self) -> bool;

    // --- reading ---

    /// False once the server has shown it lacks the owner reads (`GET
    /// /api/reading/imported` answered 404): the reading, mentions and
    /// site-settings screens then say "not available on this server" instead
    /// of showing an empty list. True until then.
    fn read_extensions_available(&self) -> bool;

    /// "Reply to a reading item": a local draft thread whose `stub_of` is
    /// `of` (spec: stubs are the reply shape), pushed like any draft. Local.
    /// Additive; the default refuses so implementors compile.
    fn create_stub(&self, of: &RemoteRef, content_md: &str) -> Result<LocalId> {
        let _ = (of, content_md);
        Err(CoreError::Other("replies aren't supported here".into()))
    }

    // --- scratch media ---
    // docs/SPEC.md § Scratch notes: an image pasted into a scratch note stays
    // on this Mac until the note is promoted (`crate::scratch_media`).

    /// Keep an image for a scratch note on this Mac and return the markdown
    /// URL that refers to it (`blyg-local:<sha256>.<ext>`). Local: no
    /// network, ever. `promote` uploads it and rewrites the reference.
    /// Additive; the default refuses so implementors compile.
    fn save_scratch_media(&self, bytes: &[u8], mime: &str) -> Result<String> {
        let _ = (bytes, mime);
        Err(CoreError::Other(
            "local images aren't supported here".into(),
        ))
    }

    /// The file behind a `blyg-local:` URL (for the previews), if it's here.
    /// Additive; the default knows none.
    fn scratch_media_file(&self, url: &str) -> Option<std::path::PathBuf> {
        let _ = url;
        None
    }

    // --- profiles ---
    // docs/SPEC.md § Profiles. Someone's public profile, from their public
    // files (manifest, blogroll, archive index; or an RSS/Atom feed),
    // fetched unauthenticated and **only when the user opens it**: never in
    // the background, never polled.

    /// The profile at `url`: a blyg origin, a post permalink, or an RSS/Atom
    /// feed URL, discovered the way the subscribe preview does it. Returns
    /// the cached copy without a request while it's fresh
    /// (`profile::PROFILE_TTL_MS`) unless `refresh`; otherwise fetches
    /// (blocking). When the fetch fails and a copy is cached, that copy comes
    /// back with `stale: true`. Connections are recomputed from local data on
    /// every call. Additive; the default refuses so implementors compile.
    fn profile(&self, url: &str, refresh: bool) -> Result<crate::profile::Profile> {
        let _ = (url, refresh);
        Err(CoreError::Other("profiles aren't supported here".into()))
    }

    /// The cached profile for `url`, whatever its age. Local, instant, no
    /// network: for showing something while `profile` runs. Additive.
    fn cached_profile(&self, url: &str) -> Option<crate::profile::Profile> {
        let _ = url;
        None
    }

    // --- about ---

    /// Whether the server serves everything Burrow reads (`GET
    /// /api/reading/imported`, studio contract 5): `Some(true)` once a
    /// reading pull found it, `None` until then.
    fn server_extensions(&self) -> Option<bool> {
        None
    }

    /// The connected server is older than blygger-studio 0.49.12 (its
    /// private API contract isn't 5.x): nothing syncs until it's updated
    /// (`CoreEvent::ServerOutdated` says when first seen).
    fn server_outdated(&self) -> bool;

    /// Whether read state syncs with the server (`GET /api/reading`
    /// advertised `readState: true`; meta key `read_sync`).
    /// For the About window.
    fn read_state_sync(&self) -> bool;

    /// Every profile fetched so far, one per origin, whatever its age.
    /// Local, no network (the composer's @-mentions read it). Additive.
    fn cached_profiles(&self) -> Vec<crate::profile::Profile> {
        Vec::new()
    }

    // --- responses --- (issue #7)

    /// Posts in the local reading list, and your own published posts, that
    /// quote, stub or fork the post `(origin, id)`: "seen in your network".
    /// One per post, the strongest relation winning (fork > stub > quote).
    /// Local, instant; a list, never a count. Additive; the default knows none.
    fn responses(&self, origin: &str, id: &str) -> Vec<Response> {
        let _ = (origin, id);
        Vec::new()
    }

    // --- reader folders ---
    // The Reader's folders of subscriptions: local only (this Mac's store),
    // never sent to the blyg and unrelated to the server's hoppers. One
    // folder per subscription. All local and instant. Additive; the
    // defaults know no folders and refuse changes, so implementors compile.

    /// Every folder, in display order.
    fn folders(&self) -> Vec<Folder> {
        Vec::new()
    }

    /// Subscription id → folder id, for each filed subscription.
    fn subscription_folders(&self) -> std::collections::HashMap<SubId, String> {
        Default::default()
    }

    /// A new folder at the end. `Rejected{400}` for an empty name,
    /// `Rejected{409}` for a name another folder has (any case).
    fn create_folder(&self, name: &str) -> Result<Folder> {
        let _ = name;
        Err(CoreError::Other("folders aren't supported here".into()))
    }

    fn rename_folder(&self, id: &str, name: &str) -> Result<()> {
        let _ = (id, name);
        Err(CoreError::Other("folders aren't supported here".into()))
    }

    /// Its subscriptions go back to unfiled.
    fn delete_folder(&self, id: &str) -> Result<()> {
        let _ = id;
        Err(CoreError::Other("folders aren't supported here".into()))
    }

    /// Move a folder to `index` in the display order (clamped).
    fn move_folder(&self, id: &str, index: usize) -> Result<()> {
        let _ = (id, index);
        Err(CoreError::Other("folders aren't supported here".into()))
    }

    /// File a subscription in a folder, or unfile it (`None`).
    fn set_subscription_folder(&self, sub_id: &SubId, folder: Option<&str>) -> Result<()> {
        let _ = (sub_id, folder);
        Err(CoreError::Other("folders aren't supported here".into()))
    }

    // --- end reader folders ---

    // --- shelf ---
    // Your own posts kept to hand at the top of the Write list, in your
    // order. Local only (this store), never sent to the blyg, and unrelated
    // to pinning a version (public and permanent). The reads are required
    // (a wrapper that forgot them showed an empty shelf, 081edf3); the
    // changes' defaults refuse them.

    /// The shelf in order (withdrawn posts left out).
    fn shelf(&self) -> Vec<LocalId>;

    /// Put a post on the shelf (at the end), or take it off. Idempotent.
    fn set_on_shelf(&self, id: &LocalId, on: bool) -> Result<()> {
        let _ = (id, on);
        Err(CoreError::Other("the shelf isn't supported here".into()))
    }

    /// Move a shelved post to `index` on the shelf (clamped).
    fn move_on_shelf(&self, id: &LocalId, index: usize) -> Result<()> {
        let _ = (id, index);
        Err(CoreError::Other("the shelf isn't supported here".into()))
    }

    // --- shelf sync --- (studio 0.61) the shelf holds anyone's posts too,
    // named as the blyg names them, and syncs with a blyg that offers it.

    /// Everything on the shelf the blyg can name, in order: your posts it
    /// holds (`~own`, server id) and others' (subscription, remote id).
    /// Reading's Shelf lens lists these.
    fn shelf_entries(&self) -> Vec<crate::shelf::Entry>;

    /// Put a post on the shelf (at the end) or take it off, by the reading
    /// row's key. Idempotent.
    fn set_entry_on_shelf(&self, entry: &crate::shelf::Entry, on: bool) -> Result<()> {
        let _ = (entry, on);
        Err(CoreError::Other("the shelf isn't supported here".into()))
    }

    /// Whether the shelf syncs with the blyg, and whether a change waits.
    fn shelf_state(&self) -> crate::shelf::SyncState;

    // --- end shelf ---

    // --- trails ---
    // Paths through posts you chose to keep (`crate::trails`): ordered
    // stops with a "why" each. Local only (this store), never sent to the
    // blyg; publishing one is an ordinary draft. Additive; the defaults know
    // no trails and refuse changes, so implementors compile.

    /// Every trail, in order, with its stop count.
    fn trails(&self) -> Vec<crate::trails::Trail> {
        Vec::new()
    }

    /// A new, empty trail. `Rejected{400}` for an empty name.
    fn create_trail(&self, name: &str) -> Result<crate::trails::Trail> {
        let _ = name;
        Err(CoreError::Other("trails aren't supported here".into()))
    }

    fn rename_trail(&self, id: &str, name: &str) -> Result<()> {
        let _ = (id, name);
        Err(CoreError::Other("trails aren't supported here".into()))
    }

    /// Delete a trail (the posts are untouched).
    fn delete_trail(&self, id: &str) -> Result<()> {
        let _ = id;
        Err(CoreError::Other("trails aren't supported here".into()))
    }

    /// A trail's stops in order; a withdrawn post's stop is a tombstone
    /// with no title.
    fn trail_stops(&self, trail: &str) -> Vec<crate::trails::TrailStop> {
        let _ = trail;
        Vec::new()
    }

    /// Add a post at the end of a trail (or update the why of one already
    /// on it); its index. A withdrawn post is refused.
    fn add_trail_stop(&self, trail: &str, stop: &crate::trails::TrailStop) -> Result<usize> {
        let _ = (trail, stop);
        Err(CoreError::Other("trails aren't supported here".into()))
    }

    fn set_trail_why(&self, trail: &str, index: usize, why: &str) -> Result<()> {
        let _ = (trail, index, why);
        Err(CoreError::Other("trails aren't supported here".into()))
    }

    fn remove_trail_stop(&self, trail: &str, index: usize) -> Result<()> {
        let _ = (trail, index);
        Err(CoreError::Other("trails aren't supported here".into()))
    }

    /// Move the stop at `from` to `to` (clamped).
    fn move_trail_stop(&self, trail: &str, from: usize, to: usize) -> Result<()> {
        let _ = (trail, from, to);
        Err(CoreError::Other("trails aren't supported here".into()))
    }

    // --- end trails ---

    // --- quote targets ---
    // Opening a quote's (or a stub's, or a fork's) original that isn't held
    // here: its public files, fetched unauthenticated with `PublicClient`
    // (never the owner token) and only when the user opens it.

    /// `{origin}items/{id}.json` as a post to show. Blocking. Additive; the
    /// default refuses so implementors compile.
    fn public_item(&self, origin: &str, id: &str) -> Result<PublicItem> {
        let _ = (origin, id);
        Err(CoreError::Other("not supported here".into()))
    }

    /// A pinned version of a post that isn't held, `{origin}items/{id}/v{n}.json`
    /// (cached forever). Refused without a request when its changelog doesn't
    /// mark `version` pinned (§8.4), as `remote_pinned` does. Additive.
    fn public_pinned(&self, origin: &str, id: &str, version: u32) -> Result<PinnedVersion> {
        let _ = (origin, id, version);
        Err(CoreError::Other("not supported here".into()))
    }

    // --- lineage counts ---
    // The `lineage-glyph` studio extension's owner reads (`crate::lineage`,
    // blygger-studio PR #53), cached in the store. `None` from any of these
    // means the node doesn't serve them (a 404, remembered for
    // `lineage::OFF_RECHECK_MS`): the UI counts from what this Mac holds
    // (`lineage::Local`). Additive; the defaults serve nothing.

    /// Glyph counts for reading entry keys (`lineage::imported_key`):
    /// cached answers younger than `max_age_ms` as they are, the rest
    /// fetched (blocking, at most `lineage::MAX_SUMMARY_KEYS` a request). A
    /// key the node didn't know is left out. Offline, the cached answers
    /// whatever their age.
    fn lineage_summaries(
        &self,
        keys: &[String],
        max_age_ms: i64,
    ) -> Option<std::collections::HashMap<String, crate::lineage::LineageSummary>> {
        let _ = (keys, max_age_ms);
        None
    }

    /// The cached glyph counts only, whatever their age. Local, instant.
    fn cached_lineage_summaries(
        &self,
        keys: &[String],
    ) -> Option<std::collections::HashMap<String, crate::lineage::LineageSummary>> {
        let _ = keys;
        None
    }

    /// One hop of lineage around `centre`, from the node (blocking; a cached
    /// answer younger than `max_age_ms` is used as it is). `None` when the
    /// node doesn't serve it or doesn't know the post.
    fn lineage_graph(
        &self,
        centre: &crate::lineage::Centre,
        max_age_ms: i64,
    ) -> Option<crate::lineage::LineageGraph> {
        let _ = (centre, max_age_ms);
        None
    }

    /// The cached graph only, whatever its age. Local, instant.
    fn cached_lineage_graph(
        &self,
        centre: &crate::lineage::Centre,
    ) -> Option<crate::lineage::LineageGraph> {
        let _ = centre;
        None
    }

    // --- browse --- following links out of an entry (studio 0.49.0;
    // `crate::browse`). Additive; the defaults classify nothing and keep no
    // log, so implementors compile.

    /// Which of `urls` name a blyg item (blocking; at most
    /// `browse::CLASSIFY_MAX` are asked at once, more are batched). Studio
    /// 0.49+: `POST /api/peek/links`; elsewhere probed with the public client
    /// and cached locally. Order follows `urls`.
    fn classify_links(&self, urls: &[String]) -> Result<Vec<crate::browse::LinkTarget>> {
        Ok(urls
            .iter()
            .map(|u| crate::browse::LinkTarget {
                url: u.clone(),
                target: None,
            })
            .collect())
    }

    /// Record one step of the private browsing log; returns its id (the next
    /// step's `parent_id`). Studio 0.49+: `POST /api/browse` (scope
    /// `reading:state`); elsewhere, or on a 403, the local store.
    fn log_browse(&self, step: &crate::browse::NewBrowseStep) -> Result<i64> {
        let _ = step;
        Err(CoreError::Other("not supported here".into()))
    }

    /// The browsing log, newest first: one trail, or every trail.
    fn browse_history(
        &self,
        trail: Option<&str>,
        offset: usize,
        limit: usize,
    ) -> Result<Vec<crate::browse::BrowseStep>> {
        let _ = (trail, offset, limit);
        Ok(Vec::new())
    }

    /// Clear one trail, or the whole log.
    fn clear_browse(&self, trail: Option<&str>) -> Result<()>;

    /// --- background --- The Background lens (studio 0.49.0): withdrawn
    /// posts only, the blygs you read (tombstones, signalled or not) newest
    /// first. `reading()` keeps hiding unsignalled tombstones; your own
    /// withdrawn posts come from `items()`. Local, instant. Additive.
    fn background_reading(&self) -> Vec<ReadingItem> {
        Vec::new()
    }

    // --- background --- studio 0.61 (contract 5.10.0): Background and
    // Foreground (`crate::background`). Gated on the capability
    // `setReadingPlacement`; all additive with defaults that offer nothing.

    /// Whether the blyg serves Background. False: no Background actions or
    /// rules; the Background lens holds withdrawn posts, as before.
    fn background_served(&self) -> bool {
        false
    }

    /// The studio's Background lens as last read (`None`: not served, or
    /// not read yet). Put it on rows with `BackgroundSet::annotate`. Local.
    fn background_set(&self) -> Option<crate::background::BackgroundSet>;

    /// The owner's choice on one row (`withdrawn`: a withdrawn post):
    /// Background, Foreground, or `None` to let the rules decide. Remote;
    /// shows at once on success (`ReadingChanged`).
    fn set_reading_placement(
        &self,
        key: &crate::background::Key,
        withdrawn: bool,
        placement: Option<crate::background::Placement>,
    ) -> Result<()> {
        let _ = (key, withdrawn, placement);
        Err(CoreError::Other("Background isn't supported here".into()))
    }

    /// Move a whole blyg (a subscription id) to Background, or back. Remote.
    fn set_blyg_background(&self, sub: &SubId, on: bool) -> Result<()> {
        let _ = (sub, on);
        Err(CoreError::Other("Background isn't supported here".into()))
    }

    /// The blyg's Background rules. `Ok(None)`: not served.
    fn background_rules(&self) -> Result<Option<crate::background::BackgroundRules>> {
        Ok(None)
    }

    /// Turn one rule on or off (a partial settings write); the rules back.
    fn save_background_rule(
        &self,
        rule: &str,
        on: bool,
    ) -> Result<crate::background::BackgroundRules> {
        let _ = (rule, on);
        Err(CoreError::Other("Background isn't supported here".into()))
    }

    // --- reader order --- the Feed's order (`Settings.readerOrder`,
    // studio 0.68). Burrow orders locally (`crate::order`); the choice is
    // the blyg's setting when it has one, else kept here.

    /// The Feed's order as last read or chosen. Local; pace-layered until
    /// told otherwise.
    fn reader_order(&self) -> crate::order::ReaderOrder {
        crate::order::ReaderOrder::default()
    }

    /// Read the blyg's `readerOrder` (and keep it). `Ok(None)`: the blyg
    /// doesn't have the setting; the local choice stands.
    fn load_reader_order(&self) -> Result<Option<crate::order::ReaderOrder>> {
        Ok(None)
    }

    /// Choose the Feed's order: kept here, and written to the blyg (that
    /// key alone) when it has the setting. The order kept back.
    fn save_reader_order(
        &self,
        order: crate::order::ReaderOrder,
    ) -> Result<crate::order::ReaderOrder> {
        let _ = order;
        Err(CoreError::Other("the reader order isn't kept here".into()))
    }

    /// Every cached changelog of someone else's post, by `(origin, remote
    /// id)`: their pins slow a post's sinking in the pace-layered Feed.
    /// Local.
    fn cached_changelogs(&self) -> std::collections::HashMap<(String, String), Vec<RemoteVersion>> {
        std::collections::HashMap::new()
    }

    // --- hoppers ---
    // The blyg's hoppers (studio's createHopper … removeHopperItem). The list
    // and memberships are local reads of what the last pull stored. A
    // hopper's own fields change on the server first (blocking, like a
    // subscription's rename); membership is local and instant, queued in
    // the outbox (offline-safe). Additive; the defaults know no hoppers and
    // refuse changes, so implementors compile.

    /// Every hopper, oldest first, with its post count. Local.
    fn hoppers(&self) -> Vec<Hopper> {
        Vec::new()
    }

    /// (subscription id, remote id) → the ids of the hoppers holding that
    /// post; your own posts are `("~own", server id)`. Local.
    fn hopper_members(&self) -> std::collections::HashMap<(SubId, RemoteId), Vec<String>> {
        Default::default()
    }

    /// Hopper id → its posts as (subscription id, remote id) in the
    /// hopper's own order: newest added first, as the studio lists them.
    /// A post put in here waits at the top until the blyg says when. Local.
    fn hopper_order(&self) -> std::collections::HashMap<String, Vec<(SubId, RemoteId)>> {
        Default::default()
    }

    /// Posts in a hopper that aren't held as reading rows (the hopper read
    /// returned them), each with its `hoppers`. Local.
    fn hopper_unheld(&self) -> Vec<ReadingItem> {
        Vec::new()
    }

    /// A new, private, empty hopper. Remote. `Rejected{400}` for an empty name.
    fn create_hopper(&self, name: &str) -> Result<Hopper> {
        let _ = name;
        Err(CoreError::Other("hoppers aren't supported here".into()))
    }

    /// Rename it, make it public or private, or set its description (an
    /// empty one clears it). Remote; the stored hopper follows the answer.
    fn update_hopper(&self, id: &str, edit: &HopperEdit) -> Result<Hopper> {
        let _ = (id, edit);
        Err(CoreError::Other("hoppers aren't supported here".into()))
    }

    /// Delete a hopper. Remote. The posts in it are untouched.
    fn delete_hopper(&self, id: &str) -> Result<()> {
        let _ = id;
        Err(CoreError::Other("hoppers aren't supported here".into()))
    }

    /// Put a post in a hopper (`on`) or take it out. Local and instant (the
    /// row's `hoppers` and the count follow at once); queued for the blyg.
    /// `sub_id` is `"~own"` with your post's server id for one of yours.
    fn set_in_hopper(
        &self,
        hopper: &str,
        sub_id: &SubId,
        remote_id: &RemoteId,
        on: bool,
    ) -> Result<()> {
        let _ = (hopper, sub_id, remote_id, on);
        Err(CoreError::Other("hoppers aren't supported here".into()))
    }

    /// --- conflicts --- An item already in conflict: (mine, theirs), the
    /// local text and the server's as last pulled, so the resolve sheet can
    /// be shown again after it was dismissed (issue #52). `None` when the
    /// item isn't in conflict. Local, instant.
    fn conflict_texts(&self, id: &LocalId) -> Option<(String, String)>;

    // --- site --- The public site's settings Studio sets up (theme, font,
    // front page, links menus, avatar, collection feeds, update alerts,
    // extensions; `crate::site`). Additive: the defaults say "not here".

    /// Read them (with the studio's extensions and update state).
    /// `NotFound` when the server has no settings route.
    fn site_config(&self) -> Result<crate::site::SiteConfig> {
        Err(CoreError::NotFound)
    }

    /// Save what differs between `next` and `base` (as last read); answers
    /// the stored settings. A refusal is `Rejected` with the server's words.
    fn save_site_config(
        &self,
        base: &crate::site::SiteConfig,
        next: &crate::site::SiteConfig,
    ) -> Result<crate::site::SiteConfig> {
        let _ = (base, next);
        Err(CoreError::Other(
            "site settings aren't supported here".into(),
        ))
    }

    /// The studio's update state as last read: local, instant. `None`
    /// until read, when not served, or with update alerts off.
    fn studio_update(&self) -> Option<crate::site::StudioUpdate> {
        None
    }

    /// The studio version (`blyg.json`'s generator) last seen, from the
    /// update state or the contacts refresh, kept across launches: local,
    /// instant. `None`: never known.
    fn studio_version(&self) -> Option<String> {
        self.studio_update().and_then(|u| u.current)
    }

    /// Read the update state again if it's older than six hours (`force`:
    /// now). Network; call it off the UI thread.
    fn refresh_studio_update(&self, force: bool) -> Result<Option<crate::site::StudioUpdate>> {
        let _ = force;
        Ok(None)
    }
    // --- end site ---

    // --- nearby --- Nearby thoughts' index and phrase search, local only
    // (nothing here touches the network). The read's default finds no
    // vectors (the app embeds again each run) and the search's runs in
    // memory; the writes are required (a backend that keeps no vectors
    // answers `Ok`).

    /// Every stored vector made by `model`.
    fn nearby_vectors(&self, model: &str) -> Vec<crate::nearby::NearbyVector> {
        let _ = model;
        Vec::new()
    }

    /// Replace the stored vectors of the posts `docs` with `rows`.
    fn put_nearby_vectors(
        &self,
        model: &str,
        docs: &[String],
        rows: &[crate::nearby::NearbyVector],
    ) -> Result<()>;

    /// Keep only `model`'s vectors of the posts in `keep`.
    fn retain_nearby_vectors(
        &self,
        model: &str,
        keep: &std::collections::HashSet<String>,
    ) -> Result<()>;

    /// The posts the rail may offer whose text contains each phrase, as
    /// `(doc key, phrase index)` (`crate::nearby`'s keys). The live
    /// backend asks the trigram indexes.
    fn nearby_phrase_hits(&self, phrases: &[String]) -> Vec<(String, usize)> {
        use crate::nearby::{offers_own, offers_reading, own_key, reading_key};
        let needles: Vec<String> = phrases.iter().map(|p| p.trim().to_lowercase()).collect();
        let subs = self.subscriptions();
        let blyg = |id: &str| {
            subs.iter()
                .any(|s| s.id == id && s.kind == SubscriptionKind::Blyg)
        };
        let mut out = Vec::new();
        for it in self.items().iter().filter(|i| offers_own(i)) {
            let text = it.content_md.to_lowercase();
            for (i, n) in needles.iter().enumerate() {
                if n.chars().count() >= 3 && text.contains(n.as_str()) {
                    out.push((own_key(&it.local_id.0), i));
                }
            }
        }
        for r in self
            .reading()
            .iter()
            .filter(|r| offers_reading(r, blyg(&r.subscription_id)))
        {
            let text = r.content_md.to_lowercase();
            for (i, n) in needles.iter().enumerate() {
                if n.chars().count() >= 3 && text.contains(n.as_str()) {
                    out.push((reading_key(&r.subscription_id, &r.remote_id), i));
                }
            }
        }
        out
    }

    // --- access-ai --- client access and AI on the blyg (studio 0.56,
    // contract 5.7; `crate::access_ai`). Remote and blocking, except
    // `access_gate` and `own_client_id`.

    /// Whether client access can be managed now, or needs the studio
    /// password first (the studio serves it to its own sign-in only).
    fn access_gate(&self) -> crate::access_ai::AccessGate {
        crate::access_ai::AccessGate::Unavailable
    }

    /// Sign in to the studio for client access (memory only, never saved).
    fn access_sign_in(&self, password: &str) -> Result<()> {
        let _ = password;
        Err(CoreError::NotFound)
    }

    /// The OAuth client id this Burrow signs in with (a browser sign-in),
    /// to mark its own grant.
    fn own_client_id(&self) -> Option<String> {
        None
    }

    /// Every grant that can act as the owner.
    fn authorizations(&self) -> Result<Vec<crate::access_ai::Authorization>> {
        Err(CoreError::NotFound)
    }

    /// Make a manual API token; it's in the answer once.
    fn create_authorization(
        &self,
        new: &crate::access_ai::NewAuthorization,
    ) -> Result<crate::access_ai::NewToken> {
        let _ = new;
        Err(CoreError::NotFound)
    }

    /// Revoke one grant.
    fn revoke_authorization(&self, id: &str) -> Result<()> {
        let _ = id;
        Err(CoreError::NotFound)
    }

    /// Revoke every client's access.
    fn revoke_all_authorizations(&self) -> Result<()> {
        Err(CoreError::NotFound)
    }

    /// The blyg's model list and providers (`GET /api/ai/models`).
    fn ai_models(&self) -> Result<crate::access_ai::AiModels> {
        Err(CoreError::NotFound)
    }

    /// The blyg's AI settings.
    fn server_ai_settings(&self) -> Result<crate::access_ai::ServerAiSettings> {
        Err(CoreError::NotFound)
    }

    /// Save the blyg's AI settings (only the keys that are `Some`).
    fn save_server_ai_settings(&self, s: &crate::access_ai::ServerAiSettings) -> Result<()> {
        let _ = s;
        Err(CoreError::NotFound)
    }

    /// Draft a change note for the post's next version on the blyg, from
    /// its working copy (pushed first). Never publishes.
    fn draft_note(&self, id: &LocalId) -> Result<crate::access_ai::DraftedNote> {
        let _ = id;
        Err(CoreError::NotFound)
    }

    /// Classify the change the post's next version makes.
    fn classify_change(&self, id: &LocalId) -> Result<crate::access_ai::ChangeClass> {
        let _ = id;
        Err(CoreError::NotFound)
    }

    /// Publish with the note's provenance and the confirmed category. A
    /// backend without them publishes as [`Backend::publish`].
    fn publish_with(
        &self,
        id: &LocalId,
        note: Option<&str>,
        extras: &crate::access_ai::PublishExtras,
    ) -> Result<PublishOutcome> {
        let _ = extras;
        self.publish(id, note)
    }
    // --- end access-ai ---
    // --- upkeep --- Studio's maintenance surfaces (`crate::upkeep`): stale
    // quotes and their refresh, your signals and activity log, and one of
    // your versions' words for a word diff. All additive with defaults.

    /// Your threads with a stale or blocking quote, as the blyg last said
    /// (local, instant; empty until asked). The list's marker and
    /// `is:stale-quote` read it.
    fn cached_stale_quotes(&self) -> Vec<crate::upkeep::ThreadFreshness> {
        Vec::new()
    }

    /// Remote: `GET /freshness`, stalest first, remembered for
    /// [`Backend::cached_stale_quotes`].
    fn stale_quotes(&self) -> Result<Vec<crate::upkeep::ThreadFreshness>> {
        Err(CoreError::Other(
            "checking quotes isn't supported here".into(),
        ))
    }

    /// Remote: one published thread's quotes, asking their blygs for their
    /// newest versions (`GET /items/{id}/freshness`).
    fn quote_freshness(&self, id: &LocalId) -> Result<crate::upkeep::ThreadFreshness> {
        let _ = id;
        Err(CoreError::Other(
            "checking quotes isn't supported here".into(),
        ))
    }

    /// Remote: republish the thread's published words with its stale quotes
    /// re-baked (`POST /items/{id}/refresh`): a new version, the same words.
    /// Refused (409) while the blyg's working copy has unpublished edits.
    fn refresh_quotes(&self, id: &LocalId, note: Option<&str>) -> Result<crate::upkeep::Refreshed> {
        let _ = (id, note);
        Err(CoreError::Other(
            "refreshing quotes isn't supported here".into(),
        ))
    }

    /// Remote: the words of one of your published versions (`GET
    /// /items/{id}/versions/{v}`). A withdrawal marker has none (empty).
    fn version_text(&self, id: &LocalId, version: u32) -> Result<String> {
        let _ = (id, version);
        Err(CoreError::Other(
            "reading an old version isn't supported here".into(),
        ))
    }

    /// Remote: one page of your thumbs (`thumb` 1 liked, -1 disliked).
    fn thumbs_page(
        &self,
        thumb: i8,
        cursor: Option<&str>,
    ) -> Result<crate::upkeep::Page<crate::upkeep::Thumb>> {
        let _ = (thumb, cursor);
        Err(CoreError::Other("signals aren't supported here".into()))
    }

    /// Remote: one page of your private activity log, newest first.
    fn interactions_page(
        &self,
        cursor: Option<&str>,
    ) -> Result<crate::upkeep::Page<crate::upkeep::Interaction>> {
        let _ = cursor;
        Err(CoreError::Other("signals aren't supported here".into()))
    }
    // --- end upkeep ---
    // --- changelog display --- studio 0.58 (contract 5.8.0): what a
    // reader sees of a changelog, and the blyg's AI summaries of one
    // (`crate::changelog`). All additive with defaults.

    /// The blyg's changelog settings. `Ok(None)`: not served (a studio
    /// before 0.58, or a backend without them).
    fn changelog_settings(&self) -> Result<Option<crate::changelog::ChangelogSettings>> {
        Ok(None)
    }

    /// Save the changelog settings (only the keys that are `Some`); the
    /// stored values back.
    fn save_changelog_settings(
        &self,
        s: &crate::changelog::ChangelogSettings,
    ) -> Result<crate::changelog::ChangelogSettings> {
        let _ = s;
        Err(CoreError::Other(
            "changelog settings aren't supported here".into(),
        ))
    }

    /// The blyg's AI summary of a held post's changelog notes. Asked only
    /// when the post is opened, never for a card.
    fn summarize_reading_changelog(
        &self,
        sub_id: &SubId,
        remote_id: &RemoteId,
    ) -> Result<crate::changelog::ChangelogSummary> {
        let _ = (sub_id, remote_id);
        Err(CoreError::Other(
            "changelog summaries aren't supported here".into(),
        ))
    }

    /// The same for one of your own published posts.
    fn summarize_own_changelog(&self, id: &LocalId) -> Result<crate::changelog::ChangelogSummary> {
        let _ = id;
        Err(CoreError::Other(
            "changelog summaries aren't supported here".into(),
        ))
    }

    /// Draft a change note that summarizes the whole history so far, ending
    /// with the next version ("summarize the full history so far").
    fn draft_note_history(&self, id: &LocalId) -> Result<crate::access_ai::DraftedNote> {
        let _ = id;
        Err(CoreError::Other(
            "your blyg needs blygger-studio 0.58 or later to summarize the history".into(),
        ))
    }
    // --- end changelog display ---
    // --- studio 0.69 --- (contract 6.2.0) the Agentic feed's editions
    // (`crate::agentic`), the shared dictionary (`crate::dictionary`) and
    // Catch up's summary on the blyg (`crate::helpers`). Each `Ok(None)`
    // is "not offered here" (the capabilities don't list it): a genuine
    // answer, so these have defaults.

    /// Remote: the agentic feeds and their names (`GET /api/agentic`).
    fn agentic_feeds(&self) -> Result<Option<Vec<crate::agentic::Feed>>> {
        Ok(None)
    }

    /// Remote: a feed's edition, each reference resolved (`GET
    /// /api/agentic/{feed}`). Burrow never writes one.
    fn agentic_edition(&self, feed: &str) -> Result<Option<crate::agentic::Edition>> {
        let _ = feed;
        Ok(None)
    }

    /// Remote: the blyg's dictionary (`GET /api/dictionary`).
    fn dictionary(&self) -> Result<Option<crate::dictionary::Dictionary>> {
        Ok(None)
    }

    /// Remote: add a word to the owner's words, or take one out: read,
    /// change, write back from that revision, and on a conflict merge onto
    /// what's there now. The dictionary as stored back.
    fn edit_dictionary(
        &self,
        edit: &crate::dictionary::Edit,
    ) -> Result<crate::dictionary::Dictionary> {
        let _ = edit;
        Err(CoreError::Other(
            "your blyg doesn't keep a dictionary".into(),
        ))
    }

    /// Remote: the blyg's summary of the conversation `entry` quotes
    /// (`POST /api/helpers/catch-up-summary`), written by its text helpers
    /// model once per set of versions.
    fn summarize_catch_up(
        &self,
        entry: &crate::shelf::Entry,
    ) -> Result<Option<crate::helpers::CatchUpSummary>> {
        let _ = entry;
        Ok(None)
    }
    // --- end studio 0.69 ---
}
