//! --- contacts --- The fake blyg's address book, what it sent, and its
//! poll status: a studio 0.56 that keeps contacts (it rewrites `@handle`
//! only when a test asks), with a sample of each delivery status and one subscription
//! whose polls fail.

use blyg_core::contacts::{Contact, ContactId, ContactInput};
use blyg_core::outbound::OutboundMention;
use blyg_core::{CoreError, Result, Subscription, SubscriptionFlag};
use chrono::{Duration, Utc};

use super::reading_seed::{LIN, OMAR, OWN_GARDENS_SERVER};

/// Your "On friction" (the reading seed has it for tests only).
const OWN_FRICTION: &str = "01J9M2AABCDEFGHJKMNPQRS";

/// The book, as the fake blyg keeps it.
pub struct Book {
    pub contacts: Vec<Contact>,
    pub outbound: Vec<OutboundMention>,
    next: u64,
    /// Every `resolve_contact` (tests).
    pub resolves: Vec<String>,
    /// Whether publish rewrites `@handle` (off: the tour and the composer
    /// tests pick a contact as a plain link; tests turn it on).
    pub rewrites: bool,
}

impl Default for Book {
    fn default() -> Self {
        let ago = |m: i64| (Utc::now() - Duration::minutes(m)).to_rfc3339();
        let out = |id: &str, item: &str, v: u32, target: &str, status: &str, tries: u32| {
            OutboundMention {
                id: id.into(),
                item_id: item.into(),
                version: v,
                target: target.into(),
                status: status.into(),
                attempts: tries,
                created: ago(60 * tries as i64 + 5),
                ..Default::default()
            }
        };
        Book {
            rewrites: false,
            contacts: vec![Contact {
                handle: "lin".into(),
                name: Some("Lin".into()),
                origin: LIN.into(),
                id: "c-lin".into(),
                ids: vec![ContactId {
                    id: "https://social.example.net/@lin".into(),
                    provenance: "self".into(),
                }],
            }],
            outbound: vec![
                OutboundMention {
                    endpoint: Some(format!("{LIN}webmention")),
                    ..out(
                        "o1",
                        OWN_GARDENS_SERVER,
                        2,
                        &format!("{LIN}t/01k211n0gardens00000000001/"),
                        "sent",
                        1,
                    )
                },
                OutboundMention {
                    next_attempt_at: Some(ago(-30)),
                    last_error: Some("the endpoint answered 503".into()),
                    ..out(
                        "o2",
                        OWN_FRICTION,
                        1,
                        "https://notes.example.org/tide/",
                        "pending",
                        2,
                    )
                },
                OutboundMention {
                    last_error: Some("no Webmention endpoint on the page".into()),
                    ..out(
                        "o3",
                        OWN_GARDENS_SERVER,
                        1,
                        &format!("{OMAR}2025/year/"),
                        "no_endpoint",
                        1,
                    )
                },
            ],
            next: 1,
            resolves: Vec::new(),
        }
    }
}

fn rejected(status: u16, message: String) -> CoreError {
    CoreError::Rejected {
        status,
        message,
        details: vec![],
    }
}

impl Book {
    fn taken(&self, handle: &str, except: Option<&str>) -> bool {
        self.contacts
            .iter()
            .any(|c| c.handle.eq_ignore_ascii_case(handle) && Some(c.id.as_str()) != except)
    }

    /// `POST /contacts`, with the studio's checks.
    pub fn create(&mut self, c: &ContactInput) -> Result<Contact> {
        c.check().map_err(|e| rejected(400, e))?;
        let c = c.tidy();
        if self.taken(&c.handle, None) {
            return Err(rejected(
                409,
                format!("@{} is already in the book", c.handle),
            ));
        }
        let made = Contact {
            handle: c.handle,
            name: Some(c.name),
            origin: c.url,
            id: format!("c-{}", self.next),
            ids: vec![],
        };
        self.next += 1;
        self.contacts.push(made.clone());
        self.contacts.sort_by(|a, b| a.handle.cmp(&b.handle));
        Ok(made)
    }

    /// `PATCH /contacts/{id}`.
    pub fn update(&mut self, id: &str, c: &ContactInput) -> Result<Contact> {
        c.check().map_err(|e| rejected(400, e))?;
        let c = c.tidy();
        if self.taken(&c.handle, Some(id)) {
            return Err(rejected(
                409,
                format!("@{} is already in the book", c.handle),
            ));
        }
        let held = self
            .contacts
            .iter_mut()
            .find(|x| x.id == id)
            .ok_or(CoreError::NotFound)?;
        held.handle = c.handle;
        held.name = Some(c.name);
        held.origin = c.url;
        let out = held.clone();
        self.contacts.sort_by(|a, b| a.handle.cmp(&b.handle));
        Ok(out)
    }

    /// `DELETE /contacts/{id}`.
    pub fn delete(&mut self, id: &str) -> Result<()> {
        let n = self.contacts.len();
        self.contacts.retain(|c| c.id != id);
        if self.contacts.len() == n {
            return Err(CoreError::NotFound);
        }
        Ok(())
    }

    /// `POST /contacts/{id}/resolve`: a page on the sample blygs lists one
    /// other address of theirs; any other page can't be read.
    pub fn resolve(&mut self, id: &str) -> Result<Contact> {
        self.resolves.push(id.to_string());
        let c = self
            .contacts
            .iter_mut()
            .find(|c| c.id == id)
            .ok_or(CoreError::NotFound)?;
        let host = crate::vm::url_host(&c.origin).unwrap_or_default();
        if !host.ends_with("example.com") && !host.ends_with("example.org") {
            return Err(rejected(
                502,
                format!("could not read {}: the page answered 404", c.origin),
            ));
        }
        let me = format!("https://social.example.net/@{}", c.handle);
        if !c.ids.iter().any(|i| i.id == me) {
            c.ids.push(ContactId {
                id: me,
                provenance: "self".into(),
            });
        }
        Ok(c.clone())
    }
}

/// The sample poll status: most polled minutes ago; Omar's feed failing.
pub fn poll(subs: &mut [Subscription]) {
    let ago = |m: i64| (Utc::now() - Duration::minutes(m)).to_rfc3339();
    for (i, s) in subs.iter_mut().enumerate() {
        s.poll.last_poll_at = Some(ago(4 + 7 * i as i64));
        if s.origin == OMAR {
            s.poll.fail_count = 3;
            s.poll.flags = vec![SubscriptionFlag {
                kind: "fetch_error".into(),
                at: ago(4),
                detail: Some("the feed answered 502".into()),
            }];
        }
    }
}
