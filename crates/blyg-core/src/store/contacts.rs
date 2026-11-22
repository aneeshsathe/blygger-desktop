//! --- contacts --- The local copy of the blyg's contact book (one meta
//! value, JSON `[Contact]`). One writer takes a list the blyg served, with
//! the prune decision (`sync/newer.rs`); a contact deleted here goes at once.

use super::Store;
use crate::api::Prune;
use crate::backend::{CoreError, Result};
use crate::contacts::Contact;
use crate::sync::newer::{keep_held, same_contact};

/// `meta`: the book as last read (JSON `[Contact]`).
const CONTACTS_META: &str = "contacts.json";

impl Store {
    /// The contact book as held here.
    pub fn contacts(&self) -> Vec<Contact> {
        self.meta(CONTACTS_META)
            .and_then(|j| serde_json::from_str(&j).ok())
            .unwrap_or_default()
    }

    /// Whether `served` lacks a contact held here (by id, else handle).
    pub fn contacts_dropped_by(&self, served: &[Contact]) -> bool {
        self.contacts()
            .iter()
            .any(|h| !served.iter().any(|c| same_contact(c, h)))
    }

    /// Keep the book the blyg served. What it lacks comes off only when
    /// `prune` allows; otherwise it stays, after the served ones.
    pub fn take_served_contacts(&self, served: Vec<Contact>, prune: Prune) -> Result<()> {
        let list = match prune {
            Prune::Allowed => served,
            Prune::Withheld => keep_held(&self.contacts(), served, same_contact),
        };
        self.write_contacts(&list)
    }

    /// A contact deleted here (by the studio's id): gone at once.
    pub fn drop_contact(&self, id: &str) -> Result<()> {
        let left: Vec<Contact> = self.contacts().into_iter().filter(|c| c.id != id).collect();
        self.write_contacts(&left)
    }

    fn write_contacts(&self, list: &[Contact]) -> Result<()> {
        let json = serde_json::to_string(list).map_err(|e| CoreError::Other(e.to_string()))?;
        self.set_meta(CONTACTS_META, &json)
    }
}
