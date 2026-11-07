//! --- nearby --- Nearby thoughts' shared shapes: a stored embedding of one
//! piece of a post, and the keys that name the posts the rail can offer.
//! The vectors live in the store (`nearby_vectors`, schema v20), the
//! embedding itself happens in the app (`blyg-embed`). Only what this Mac
//! holds is ever indexed: your own published posts and current posts from
//! blyg subscriptions. A withdrawn post's vectors are deleted with it.

use crate::model::{Item, ReadingItem, Status};

/// One embedded piece of a post.
#[derive(Debug, Clone, PartialEq)]
pub struct NearbyVector {
    /// [`own_key`] or [`reading_key`].
    pub doc: String,
    /// The piece's position in the post (0 is its title).
    pub chunk: u32,
    /// A hash of the piece's text ([`text_hash`]): a piece whose text is
    /// unchanged isn't embedded again.
    pub hash: String,
    /// Unit length.
    pub vec: Vec<f32>,
}

/// The key of one of your own posts.
pub fn own_key(local_id: &str) -> String {
    format!("own:{local_id}")
}

/// The key of a post held from a subscription.
pub fn reading_key(subscription_id: &str, remote_id: &str) -> String {
    format!("reading:{subscription_id}/{remote_id}")
}

/// Whether the rail may offer `it`: published, public, with a server id
/// to link to. A withdrawn post never.
pub fn offers_own(it: &Item) -> bool {
    it.status == Status::Public && it.server_id.is_some()
}

/// Whether the rail may offer `r`: a current post from a blyg (a link or
/// quote names it by id), never a withdrawn one, even with a pinned
/// version kept.
pub fn offers_reading(r: &ReadingItem, blyg: bool) -> bool {
    blyg && r.state == "current"
}

/// A short, stable hash of a piece's text (hex, the first 16 bytes of its
/// sha256).
pub fn text_hash(text: &str) -> String {
    use sha2::{Digest, Sha256};
    Sha256::digest(text.as_bytes())[..16]
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keys_and_hashes() {
        assert_eq!(own_key("l1"), "own:l1");
        assert_eq!(reading_key("s", "r"), "reading:s/r");
        assert_eq!(text_hash("a"), text_hash("a"));
        assert_ne!(text_hash("a"), text_hash("b"));
        assert_eq!(text_hash("a").len(), 32);
    }
}
