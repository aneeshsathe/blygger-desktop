//! --- contacts --- Studio 0.55–0.56's address book, managed from Burrow:
//!
//! - Blyg › Contacts… (the command palette too): the book. Search, add,
//!   edit, remove, and "Check page" (`POST /contacts/{id}/resolve`, which
//!   reads the person's page for the identifiers it vouches for). An add
//!   checks the page as Studio does, best effort.
//! - Post › Add Author to Contacts…: the reading post in front of you; its
//!   author's own name and address when the post gives them, else the
//!   blyg's. One already in the book is named, not added twice.
//! - After subscribing to a blyg, the offer to add it (its manifest's
//!   author), as Studio's subscribe sheet makes it.
//! - The Mentions screen's Sent side (`sent.rs`) and the Subscriptions
//!   screen's poll status (`poll.rs`).
//!
//! A child module of `reading` (it uses the reading screens' helpers); the
//! sheet is a main-window sheet (`Sheet::Contacts`). Hooks elsewhere are
//! marked `// --- contacts ---`.

use blyg_core::contacts::{Contact, ContactInput, ContactSeed, held, matching};
use blyg_core::{ReadingItem, SubscriptionKind};
use gpui_kit::base::input::{InputEvent, InputState};
use gpui_kit::*;

use super::View;
use crate::app::{MainView, Sheet};

mod demo;
pub(crate) mod poll;
pub(crate) mod sent;
mod sheet;
#[cfg(test)]
mod tests;

gpui_kit::actions!(blygger, [ShowContacts, AddAuthorToContacts]);

/// What the Contacts sheet says when the blyg keeps no contacts.
pub(crate) const NOT_SERVED: &str =
    "Your blyg doesn't keep contacts. They need blygger-studio 0.56 or later.";

/// The main window's contacts state: the Mentions screen's Sent side.
#[derive(Default)]
pub(crate) struct State {
    pub sent: sent::Sent,
}

/// The Contacts sheet: the book, or (an offer) only the add form.
pub(crate) struct ContactsSheet {
    pub focus: FocusHandle,
    pub search: Entity<InputState>,
    /// The book as last read (`Backend::contacts`).
    pub book: Vec<Contact>,
    /// The highlighted row of the filtered list.
    pub sel: usize,
    /// Whether the blyg keeps contacts (`None`: still asking).
    pub served: Option<bool>,
    pub form: Option<Form>,
    /// A contact whose Remove was clicked once (click again).
    pub confirm_remove: Option<String>,
    /// A contact whose page is being checked.
    pub checking: Option<String>,
    pub error: Option<String>,
    /// Opened as an offer (after subscribing, or for a post's author):
    /// only the form, under this heading and question; done closes it.
    pub offer: Option<Offer>,
}

#[derive(Clone)]
pub(crate) struct Offer {
    pub heading: String,
    pub question: Option<String>,
}

/// The add / edit form.
pub(crate) struct Form {
    /// The contact being edited (else a new one).
    pub editing: Option<String>,
    pub handle: Entity<InputState>,
    pub name: Entity<InputState>,
    pub url: Entity<InputState>,
    pub error: Option<String>,
    pub busy: bool,
}

impl Form {
    fn input(&self, cx: &App) -> ContactInput {
        ContactInput {
            handle: self.handle.read(cx).value().to_string(),
            name: self.name.read(cx).value().to_string(),
            url: self.url.read(cx).value().to_string(),
        }
    }

    fn fields(&self) -> [&Entity<InputState>; 3] {
        [&self.handle, &self.name, &self.url]
    }
}

impl ContactsSheet {
    /// The book filtered by the search box, handles that start with it first.
    pub(crate) fn shown<'a>(&'a self, cx: &App) -> Vec<&'a Contact> {
        let q = self.search.read(cx).value().to_string();
        matching(&self.book, &q)
    }
}

/// Where an author can be found, for the offer: the post's own author
/// name and `https:` address, else the blyg's title and site (Studio's
/// `authorSeed`; an RSS feed's site is its host).
pub(crate) fn author_seed(row: &ReadingItem, kind: Option<SubscriptionKind>) -> ContactSeed {
    let site = match kind {
        Some(SubscriptionKind::Rss) => url::Url::parse(&row.origin)
            .ok()
            .map(|u| format!("{}/", u.origin().ascii_serialization()))
            .unwrap_or_else(|| row.origin.clone()),
        _ => row.origin.clone(),
    };
    ContactSeed::from_author(row.author.as_ref(), &row.subscription_title, &site)
}

impl MainView {
    /// Hook: the actions on the root element.
    pub(crate) fn contacts_actions(
        &self,
        d: Stateful<Div>,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        d.on_action(
            cx.listener(|this, _: &ShowContacts, window, cx| {
                this.toggle_contacts_sheet(window, cx)
            }),
        )
        .on_action(cx.listener(|this, _: &AddAuthorToContacts, window, cx| {
            this.add_author_to_contacts(window, cx)
        }))
    }

    /// Blyg › Contacts…: open the book, or close it.
    pub(crate) fn toggle_contacts_sheet(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if matches!(self.sheet, Some(Sheet::Contacts(_))) {
            return self.close_contacts_sheet(window, cx);
        }
        self.open_contacts_sheet(None, None, window, cx);
    }

    /// Open the sheet: the book, or with `offer` only the form, seeded.
    fn open_contacts_sheet(
        &mut self,
        offer: Option<Offer>,
        seed: Option<ContactSeed>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let search = cx
            .new(|cx| InputState::new(window, cx).placeholder("Search by handle, name or address"));
        let sub = cx.subscribe_in(&search, window, |this, _, ev, window, cx| match ev {
            InputEvent::Change => {
                if let Some(Sheet::Contacts(s)) = this.sheet.as_mut() {
                    s.sel = 0;
                    s.confirm_remove = None;
                }
                cx.notify();
            }
            InputEvent::PressEnter { .. } => this.edit_selected_contact(window, cx),
            _ => {}
        });
        self._subs.push(sub);
        let focus = cx.focus_handle();
        let book = self.backend.contacts();
        let served = self.backend.keeps_contacts().then_some(true);
        let form = seed.map(|s| self.contact_form(None, s.input(), window, cx));
        match &form {
            Some(f) => f.handle.update(cx, |s, cx| s.focus(window, cx)),
            None => search.update(cx, |s, cx| s.focus(window, cx)),
        }
        self.sheet_gen += 1;
        self.sheet = Some(Sheet::Contacts(Box::new(ContactsSheet {
            focus,
            search,
            book,
            sel: 0,
            served,
            form,
            confirm_remove: None,
            checking: None,
            error: None,
            offer,
        })));
        cx.notify();
        self.reload_contacts(cx);
    }

    /// Read the book from the blyg again (in the background).
    fn reload_contacts(&mut self, cx: &mut Context<Self>) {
        let backend = self.backend.clone();
        let task = cx.background_spawn(async move {
            let r = backend.refresh_contacts();
            (r, backend.contacts(), backend.keeps_contacts())
        });
        cx.spawn(async move |this, cx| {
            let (r, book, served) = task.await;
            let _ = this.update(cx, |v, cx| {
                if let Some(Sheet::Contacts(s)) = v.sheet.as_mut() {
                    s.book = book;
                    s.served = Some(served);
                    s.sel = s.sel.min(s.book.len().saturating_sub(1));
                    if let Err(e) = r
                        && served
                    {
                        s.error = Some(format!("Couldn't read your contacts: {e}"));
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }

    pub(crate) fn close_contacts_sheet(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.sheet = None;
        if self.reading.view == View::Posts {
            self.focus_after_sheet(window, cx);
        } else {
            window.focus(&self.reading.focus, cx);
        }
        cx.notify();
    }

    /// A form with `input` in its fields; ⏎ in any of them saves.
    fn contact_form(
        &mut self,
        editing: Option<String>,
        input: ContactInput,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Form {
        let mut field =
            |ph: &'static str, value: String, window: &mut Window, cx: &mut Context<Self>| {
                let e = cx.new(|cx| {
                    let mut s = InputState::new(window, cx).placeholder(ph);
                    s.set_value(value, window, cx);
                    s
                });
                let sub = cx.subscribe_in(&e, window, |this, _, ev, window, cx| {
                    if let InputEvent::PressEnter { .. } = ev {
                        this.save_contact(window, cx);
                    }
                });
                self._subs.push(sub);
                e
            };
        let handle = field("lin", input.handle, window, cx);
        let name = field("Lin Okafor", input.name, window, cx);
        let url = field("https://", input.url, window, cx);
        Form {
            editing,
            handle,
            name,
            url,
            error: None,
            busy: false,
        }
    }

    /// "+ Add contact": an empty form.
    pub(crate) fn new_contact(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let form = self.contact_form(None, ContactInput::default(), window, cx);
        form.handle.update(cx, |s, cx| s.focus(window, cx));
        if let Some(Sheet::Contacts(s)) = self.sheet.as_mut() {
            s.form = Some(form);
            s.confirm_remove = None;
        }
        cx.notify();
    }

    /// Edit the highlighted contact (⏎ in the search box).
    fn edit_selected_contact(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(Sheet::Contacts(s)) = self.sheet.as_ref() else {
            return;
        };
        if let Some(id) = s.shown(cx).get(s.sel).map(|c| c.id.clone()) {
            self.edit_contact(id, window, cx);
        }
    }

    pub(crate) fn edit_contact(&mut self, id: String, window: &mut Window, cx: &mut Context<Self>) {
        let Some(Sheet::Contacts(s)) = self.sheet.as_ref() else {
            return;
        };
        let Some(c) = s.book.iter().find(|c| c.id == id).cloned() else {
            return;
        };
        let input = ContactInput {
            handle: c.handle.clone(),
            name: c.display_name().to_string(),
            url: c.origin.clone(),
        };
        let form = self.contact_form(Some(id), input, window, cx);
        form.name.update(cx, |s, cx| s.focus(window, cx));
        if let Some(Sheet::Contacts(s)) = self.sheet.as_mut() {
            s.form = Some(form);
            s.confirm_remove = None;
        }
        cx.notify();
    }

    /// esc in the form: back to the list (an offer closes: "not now").
    pub(crate) fn cancel_contact_form(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(Sheet::Contacts(s)) = self.sheet.as_mut() else {
            return;
        };
        if s.offer.is_some() || s.form.is_none() {
            return self.close_contacts_sheet(window, cx);
        }
        s.form = None;
        let search = s.search.clone();
        search.update(cx, |s, cx| s.focus(window, cx));
        cx.notify();
    }

    /// Tab / ⇧Tab between the form's fields.
    pub(crate) fn contact_form_tab(
        &mut self,
        back: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(Sheet::Contacts(s)) = self.sheet.as_ref() else {
            return;
        };
        let Some(f) = s.form.as_ref() else { return };
        let fields = f.fields();
        let at = fields
            .iter()
            .position(|e| e.read(cx).focus_handle(cx).is_focused(window))
            .unwrap_or(0);
        let next = if back { (at + 2) % 3 } else { (at + 1) % 3 };
        let e = fields[next].clone();
        e.update(cx, |s, cx| s.focus(window, cx));
    }

    /// ⏎ / "Save": add or change the contact, checking it first as the
    /// studio would. A new one's page is then checked, best effort.
    pub(crate) fn save_contact(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(Sheet::Contacts(s)) = self.sheet.as_mut() else {
            return;
        };
        let Some(f) = s.form.as_mut() else { return };
        if f.busy {
            return;
        }
        let input = f.input(cx);
        if let Err(e) = input.check() {
            f.error = Some(e);
            cx.notify();
            return;
        }
        f.busy = true;
        f.error = None;
        let editing = f.editing.clone();
        let added = editing.is_none();
        let offer = s.offer.is_some();
        cx.notify();
        let backend = self.backend.clone();
        let task = cx.background_spawn(async move {
            match &editing {
                Some(id) => backend.update_contact(id, &input),
                None => {
                    let made = backend.create_contact(&input)?;
                    // Best effort: a page that can't be read leaves a
                    // contact that still works (Studio's `saveContact`).
                    Ok(backend.resolve_contact(&made.id).unwrap_or(made))
                }
            }
        });
        cx.spawn_in(window, async move |this, cx| {
            let r = task.await;
            let _ = this.update_in(cx, |v, window, cx| {
                let Some(Sheet::Contacts(s)) = v.sheet.as_mut() else {
                    return;
                };
                match r {
                    Ok(c) => {
                        s.form = None;
                        s.book = v.backend.contacts();
                        if !s.book.iter().any(|b| b.id == c.id) {
                            s.book.push(c.clone());
                        }
                        let what = if added {
                            format!("Added @{} to your contacts", c.handle)
                        } else {
                            format!("Saved @{}", c.handle)
                        };
                        if offer {
                            v.close_contacts_sheet(window, cx);
                        } else {
                            let search = s.search.clone();
                            search.update(cx, |s, cx| s.focus(window, cx));
                        }
                        v.show_toast(
                            what,
                            added.then(|| "Type @ while writing to mention them".into()),
                            cx,
                        );
                    }
                    Err(e) => {
                        if let Some(f) = s.form.as_mut() {
                            // Stays open: a handle already taken is fixed
                            // here, not by starting over.
                            f.busy = false;
                            f.error = Some(contact_error(&e));
                        }
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }

    /// Remove (click twice).
    pub(crate) fn remove_contact(&mut self, id: String, cx: &mut Context<Self>) {
        let Some(Sheet::Contacts(s)) = self.sheet.as_mut() else {
            return;
        };
        if s.confirm_remove.as_deref() != Some(id.as_str()) {
            s.confirm_remove = Some(id);
            cx.notify();
            return;
        }
        s.confirm_remove = None;
        let handle = s
            .book
            .iter()
            .find(|c| c.id == id)
            .map(|c| c.handle.clone())
            .unwrap_or_default();
        let backend = self.backend.clone();
        let task = cx.background_spawn(async move { backend.delete_contact(&id) });
        cx.spawn(async move |this, cx| {
            let r = task.await;
            let _ = this.update(cx, |v, cx| {
                let book = v.backend.contacts();
                if let Some(Sheet::Contacts(s)) = v.sheet.as_mut() {
                    match &r {
                        Ok(()) => {
                            s.book = book;
                            s.sel = s.sel.min(s.book.len().saturating_sub(1));
                        }
                        Err(e) => s.error = Some(contact_error(e)),
                    }
                }
                if r.is_ok() {
                    v.show_toast(
                        format!("Removed @{handle}"),
                        Some("Posts already published keep the link".into()),
                        cx,
                    );
                }
                cx.notify();
            });
        })
        .detach();
    }

    /// "Check page": the blyg reads the contact's page for the identifiers
    /// it vouches for (`rel="me"`).
    pub(crate) fn check_contact(&mut self, id: String, cx: &mut Context<Self>) {
        let Some(Sheet::Contacts(s)) = self.sheet.as_mut() else {
            return;
        };
        if s.checking.is_some() {
            return;
        }
        s.checking = Some(id.clone());
        s.error = None;
        cx.notify();
        let backend = self.backend.clone();
        let task = cx.background_spawn(async move { backend.resolve_contact(&id) });
        cx.spawn(async move |this, cx| {
            let r = task.await;
            let _ = this.update(cx, |v, cx| {
                let book = v.backend.contacts();
                let msg = match &r {
                    Ok(c) => match c.checked_ids().len() {
                        0 => format!("Checked @{}'s page: it lists no other addresses", c.handle),
                        1 => format!("Checked @{}'s page: 1 other address", c.handle),
                        n => format!("Checked @{}'s page: {n} other addresses", c.handle),
                    },
                    Err(e) => contact_error(e),
                };
                if let Some(Sheet::Contacts(s)) = v.sheet.as_mut() {
                    s.checking = None;
                    match r {
                        Ok(c) => {
                            s.book = book;
                            if let Some(b) = s.book.iter_mut().find(|b| b.id == c.id) {
                                *b = c;
                            }
                        }
                        Err(_) => s.error = Some(msg.clone()),
                    }
                }
                v.show_toast(msg, None, cx);
                cx.notify();
            });
        })
        .detach();
    }

    /// Post › Add Author to Contacts…: the reading post in front of you.
    pub(crate) fn add_author_to_contacts(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let row = self.front_reading_row();
        let Some(row) = row else {
            return self.show_toast(
                "Open or pick a post in Reading first",
                Some("Its author goes in your contacts".into()),
                cx,
            );
        };
        let kind = self
            .reading
            .subs
            .iter()
            .chain(self.backend.subscriptions().iter())
            .find(|s| s.id == row.subscription_id)
            .map(|s| s.kind);
        let seed = author_seed(&row, kind);
        self.offer_contact(
            seed,
            Offer {
                heading: "Add author to contacts".into(),
                question: None,
            },
            window,
            cx,
        );
    }

    /// The post in front of you in Reading: the open one, else the
    /// selected row.
    fn front_reading_row(&self) -> Option<ReadingItem> {
        if self.reading.view != View::Reading {
            return None;
        }
        if let Some(o) = self.reading.opened.as_ref() {
            return Some(o.item.clone());
        }
        let k = self.reading.sel.clone()?;
        self.reading
            .rows
            .iter()
            .find(|r| super::vm::key(r) == k)
            .cloned()
    }

    /// Offer to add `seed`: named when the book holds it already, refused
    /// when the blyg keeps no contacts.
    fn offer_contact(
        &mut self,
        seed: ContactSeed,
        offer: Offer,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.backend.keeps_contacts() {
            return self.show_toast(NOT_SERVED, None, cx);
        }
        if let Some(c) = held(&self.backend.contacts(), &seed.url, None) {
            return self.show_toast(
                format!("Already in your contacts as @{}", c.handle),
                None,
                cx,
            );
        }
        self.open_contacts_sheet(Some(offer), Some(seed), window, cx);
    }

    /// Hook (subscribing, `reading/subscriptions.rs`): after subscribing to
    /// a blyg by hand, offer to add it to your contacts, as Studio does.
    /// Nothing when the blyg keeps no contacts or the book has it already.
    pub(crate) fn offer_contact_after_subscribe(
        &mut self,
        sub: &blyg_core::Subscription,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if sub.kind != SubscriptionKind::Blyg || !self.backend.keeps_contacts() {
            return;
        }
        let (origin, title) = (sub.origin.clone(), sub.title.clone());
        let backend = self.backend.clone();
        let task = cx.background_spawn(async move { backend.contact_seed(&origin, &title) });
        let title = sub.title.clone();
        cx.spawn_in(window, async move |this, cx| {
            let Some(seed) = task.await else { return };
            let _ = this.update_in(cx, |v, window, cx| {
                // Not over another sheet the owner opened meanwhile.
                if v.sheet.is_some() || v.reading.sheet.is_some() {
                    return;
                }
                v.open_contacts_sheet(
                    Some(Offer {
                        heading: format!("Subscribed to {title}"),
                        question: Some("Add this blyg to your contacts?".into()),
                    }),
                    Some(seed),
                    window,
                    cx,
                );
            });
        })
        .detach();
    }
}

/// An error as the sheet shows it: the studio's own words when it gave any.
fn contact_error(e: &blyg_core::CoreError) -> String {
    match e {
        blyg_core::CoreError::NotFound => NOT_SERVED.to_string(),
        blyg_core::CoreError::Rejected { message, .. } if !message.is_empty() => {
            let m = message.trim();
            let mut c = m.chars();
            match c.next() {
                Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
                None => m.to_string(),
            }
        }
        e => format!("Couldn't save it: {e}"),
    }
}
