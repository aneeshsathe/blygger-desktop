//! --- contacts --- The Contacts sheet, Add Author, the subscribe offer,
//! Mentions › Sent and the poll status, in a headless window on a
//! zero-latency FakeBackend (a studio 0.56 that keeps contacts).

use std::sync::Arc;

use blyg_core::config::MemoryTokenStore;
use blyg_core::{Backend, ConfigStore, Kind};
use gpui_kit::{Entity, TestAppContext, VisualTestContext};

use super::super::View;
use super::{AddAuthorToContacts, ShowContacts};
use crate::app::{MainView, Sheet};
use crate::fake::reading_seed::*;
use crate::fake::{FakeBackend, Timing};
use crate::prefs::Prefs;

const CONNECTED: &str = "# test config\nblyg-url = https://blyg.example.com\n";

fn setup(cx: &mut TestAppContext) -> (Entity<MainView>, Arc<FakeBackend>, &mut VisualTestContext) {
    let prefs = Prefs::from_config(ConfigStore::in_memory(CONNECTED).config());
    cx.update(|cx| {
        gpui_kit::init(cx);
        crate::app::bind_keys(cx);
        crate::settings::init(
            ConfigStore::in_memory(CONNECTED),
            Arc::new(MemoryTokenStore::default()),
            None,
            cx,
        );
    });
    let fake = Arc::new(FakeBackend::with_timing(Timing::instant()).without_media_cache());
    let backend: Arc<dyn Backend> = fake.clone();
    let f2 = fake.clone();
    let (view, cx) = cx.add_window_view(move |window, cx| {
        MainView::new(
            backend,
            Some(f2),
            prefs,
            std::time::Instant::now(),
            window,
            cx,
        )
    });
    cx.run_until_parked();
    (view, fake, cx)
}

fn settle(cx: &mut VisualTestContext) {
    for _ in 0..20 {
        cx.run_until_parked();
        std::thread::sleep(std::time::Duration::from_millis(2));
    }
}

/// The open Contacts sheet's handles (`None`: not open).
fn sheet_handles(view: &Entity<MainView>, cx: &mut VisualTestContext) -> Option<Vec<String>> {
    view.read_with(cx, |v, cx| match &v.sheet {
        Some(Sheet::Contacts(s)) => Some(s.shown(cx).iter().map(|c| c.handle.clone()).collect()),
        _ => None,
    })
}

/// Fill the open form's fields.
fn fill(view: &Entity<MainView>, cx: &mut VisualTestContext, handle: &str, name: &str, url: &str) {
    view.update_in(cx, |v, window, cx| {
        let Some(Sheet::Contacts(s)) = v.sheet.as_ref() else {
            panic!("no contacts sheet");
        };
        let f = s.form.as_ref().expect("a form");
        for (e, t) in [(&f.handle, handle), (&f.name, name), (&f.url, url)] {
            let t = t.to_string();
            e.clone().update(cx, |s, cx| s.set_value(t, window, cx));
        }
    });
}

fn form_error(view: &Entity<MainView>, cx: &mut VisualTestContext) -> Option<String> {
    view.read_with(cx, |v, _| match &v.sheet {
        Some(Sheet::Contacts(s)) => s.form.as_ref().and_then(|f| f.error.clone()),
        _ => None,
    })
}

fn form_values(
    view: &Entity<MainView>,
    cx: &mut VisualTestContext,
) -> Option<(String, String, String)> {
    view.read_with(cx, |v, cx| match &v.sheet {
        Some(Sheet::Contacts(s)) => s.form.as_ref().map(|f| {
            let i = f.input(cx);
            (i.handle, i.name, i.url)
        }),
        _ => None,
    })
}

/// The toast showing now (as a list, for `any`).
fn toasts(view: &Entity<MainView>, cx: &mut VisualTestContext) -> Vec<String> {
    view.read_with(cx, |v, _| {
        v.toast
            .as_ref()
            .map(|t| t.text.to_string())
            .into_iter()
            .collect()
    })
}

#[gpui_kit::test]
fn the_book_opens_from_its_action_and_adds_edits_and_removes(cx: &mut TestAppContext) {
    let (view, fake, cx) = setup(cx);
    cx.dispatch_action(ShowContacts);
    settle(cx);
    assert_eq!(sheet_handles(&view, cx), Some(vec!["lin".to_string()]));

    // Add: a bad handle is named before any round trip.
    view.update_in(cx, |v, window, cx| v.new_contact(window, cx));
    fill(&view, cx, "-rue", "Rue", "https://rue.blyg.example.com/");
    view.update_in(cx, |v, window, cx| v.save_contact(window, cx));
    settle(cx);
    assert!(form_error(&view, cx).unwrap().contains("letters, digits"));
    // A handle already in the book: the studio's words, the form kept.
    fill(&view, cx, "lin", "Rue", "https://rue.blyg.example.com/");
    view.update_in(cx, |v, window, cx| v.save_contact(window, cx));
    settle(cx);
    assert_eq!(
        form_error(&view, cx).as_deref(),
        Some("@lin is already in the book")
    );
    fill(&view, cx, "@rue", "Rue", "https://rue.blyg.example.com/");
    view.update_in(cx, |v, window, cx| v.save_contact(window, cx));
    settle(cx);
    assert_eq!(
        sheet_handles(&view, cx),
        Some(vec!["lin".to_string(), "rue".to_string()])
    );
    let rue = fake
        .contacts()
        .into_iter()
        .find(|c| c.handle == "rue")
        .expect("added");
    assert_eq!(
        rue.checked_ids(),
        ["https://social.example.net/@rue"],
        "its page was checked"
    );
    assert!(
        toasts(&view, cx)
            .iter()
            .any(|t| t == "Added @rue to your contacts")
    );

    // Search narrows the list.
    view.update_in(cx, |v, window, cx| {
        if let Some(Sheet::Contacts(s)) = v.sheet.as_ref() {
            let search = s.search.clone();
            search.update(cx, |s, cx| s.set_value("ru", window, cx));
        }
    });
    settle(cx);
    assert_eq!(sheet_handles(&view, cx), Some(vec!["rue".to_string()]));

    // Edit: the form holds the contact; the change is saved.
    view.update_in(cx, |v, window, cx| {
        v.edit_contact(rue.id.clone(), window, cx)
    });
    assert_eq!(
        form_values(&view, cx),
        Some((
            "rue".into(),
            "Rue".into(),
            "https://rue.blyg.example.com/".into()
        ))
    );
    fill(
        &view,
        cx,
        "rue",
        "Rue Marlow",
        "https://rue.blyg.example.com/",
    );
    view.update_in(cx, |v, window, cx| v.save_contact(window, cx));
    settle(cx);
    assert_eq!(
        fake.contacts()
            .iter()
            .find(|c| c.id == rue.id)
            .unwrap()
            .name
            .as_deref(),
        Some("Rue Marlow")
    );

    // Remove takes two clicks.
    view.update_in(cx, |v, _, cx| v.remove_contact(rue.id.clone(), cx));
    settle(cx);
    assert_eq!(fake.contacts().len(), 2, "one click only asks");
    view.update_in(cx, |v, _, cx| v.remove_contact(rue.id.clone(), cx));
    settle(cx);
    assert_eq!(fake.contacts().len(), 1);
    assert!(toasts(&view, cx).iter().any(|t| t == "Removed @rue"));

    // The same action closes it.
    cx.dispatch_action(ShowContacts);
    settle(cx);
    assert_eq!(sheet_handles(&view, cx), None);
}

#[gpui_kit::test]
fn checking_a_page_that_cant_be_read_says_why(cx: &mut TestAppContext) {
    let (view, fake, cx) = setup(cx);
    fake.create_contact(&blyg_core::contacts::ContactInput {
        handle: "sam".into(),
        name: "Sam".into(),
        url: "https://sam.social.test/".into(),
    })
    .unwrap();
    cx.dispatch_action(ShowContacts);
    settle(cx);
    let id = fake
        .contacts()
        .into_iter()
        .find(|c| c.handle == "sam")
        .unwrap()
        .id;
    view.update_in(cx, |v, _, cx| v.check_contact(id, cx));
    settle(cx);
    let err = view.read_with(cx, |v, _| match &v.sheet {
        Some(Sheet::Contacts(s)) => s.error.clone(),
        _ => None,
    });
    assert_eq!(
        err.as_deref(),
        Some("Could not read https://sam.social.test/: the page answered 404")
    );
}

#[gpui_kit::test]
fn add_author_offers_the_post_author_once(cx: &mut TestAppContext) {
    let (view, fake, cx) = setup(cx);
    view.update_in(cx, |v, window, cx| v.show_view(View::Reading, window, cx));
    settle(cx);
    // Nothing picked: says so.
    view.update(cx, |v, _| {
        v.reading.sel = None;
        v.reading.opened = None;
    });
    cx.dispatch_action(AddAuthorToContacts);
    settle(cx);
    assert!(
        toasts(&view, cx)
            .iter()
            .any(|t| t == "Open or pick a post in Reading first")
    );

    // Ada's post: not in the book, so the form opens with her name and blyg.
    let key = view.read_with(cx, |v, _| {
        v.reading_rows()
            .iter()
            .find(|r| r.remote_id == ADA_FINISHED)
            .map(crate::app::reading::vm::key)
            .expect("Ada's post shown")
    });
    view.update_in(cx, |v, window, cx| v.open_reading(key, window, cx));
    settle(cx);
    cx.dispatch_action(AddAuthorToContacts);
    settle(cx);
    assert_eq!(
        form_values(&view, cx),
        Some(("ada".into(), "Ada".into(), ADA.into()))
    );
    view.update_in(cx, |v, window, cx| v.save_contact(window, cx));
    settle(cx);
    assert_eq!(sheet_handles(&view, cx), None, "an offer closes once saved");
    assert!(fake.contacts().iter().any(|c| c.handle == "ada"));

    // Again: already there, named.
    cx.dispatch_action(AddAuthorToContacts);
    settle(cx);
    assert!(
        toasts(&view, cx)
            .iter()
            .any(|t| t == "Already in your contacts as @ada")
    );
    assert_eq!(sheet_handles(&view, cx), None);
}

#[gpui_kit::test]
fn subscribing_to_a_blyg_offers_to_add_it(cx: &mut TestAppContext) {
    let (view, fake, cx) = setup(cx);
    view.update_in(cx, |v, window, cx| {
        v.show_view(View::Subscriptions, window, cx);
        v.open_subscribe(window, cx);
    });
    let input = view.read_with(cx, |v, _| match &v.reading.sheet {
        Some(super::super::RSheet::Subscribe { input, .. }) => input.clone(),
        _ => panic!("no subscribe sheet"),
    });
    view.update_in(cx, |_, window, cx| {
        input.update(cx, |s, cx| {
            s.set_value("https://kit.blyg.example.com/", window, cx)
        })
    });
    // ⏎ previews, ⏎ again subscribes.
    view.update_in(cx, |v, window, cx| v.subscribe_enter(window, cx));
    settle(cx);
    view.update_in(cx, |v, window, cx| v.subscribe_enter(window, cx));
    settle(cx);
    let (heading, question) = view.read_with(cx, |v, _| match &v.sheet {
        Some(Sheet::Contacts(s)) => {
            let o = s.offer.clone().expect("an offer");
            (o.heading, o.question)
        }
        _ => panic!("no offer"),
    });
    assert_eq!(heading, "Subscribed to Kit");
    assert_eq!(question.as_deref(), Some("Add this blyg to your contacts?"));
    assert_eq!(
        form_values(&view, cx),
        Some((
            "kit".into(),
            "Kit".into(),
            "https://kit.blyg.example.com/".into()
        ))
    );
    // "not now" closes it and adds nothing.
    view.update_in(cx, |v, window, cx| v.cancel_contact_form(window, cx));
    assert_eq!(sheet_handles(&view, cx), None);
    assert_eq!(fake.contacts().len(), 1);
}

#[gpui_kit::test]
fn mentions_show_what_was_sent_with_its_status(cx: &mut TestAppContext) {
    let (view, _, cx) = setup(cx);
    view.update_in(cx, |v, window, cx| v.show_view(View::Mentions, window, cx));
    settle(cx);
    // Received follows the blyg's own group order.
    let order = view.read_with(cx, |v, _| v.contacts.sent.group_order.clone());
    assert!(!order.is_empty());
    view.update_in(cx, |v, _, cx| v.set_mentions_side(true, cx));
    settle(cx);
    let s = view.read_with(cx, |v, _| v.sent_screen_strings());
    for want in [
        "sent",
        "pending",
        "no endpoint",
        "no Webmention endpoint on the page",
    ] {
        assert!(s.iter().any(|x| x == want), "{want} in {s:?}");
    }
    assert!(s.iter().any(|x| x.starts_with("v2 of “")), "{s:?}");
    assert!(
        s.iter().any(|x| x.starts_with("2 tries · trying again ")),
        "{s:?}"
    );
    // The sample blyg has a site URL: no warning.
    assert!(!view.read_with(cx, |v, _| v.contacts.sent.no_site_url));
}

#[gpui_kit::test]
fn reply_on_a_response_stubs_its_source_when_held(cx: &mut TestAppContext) {
    let (view, fake, cx) = setup(cx);
    view.update_in(cx, |v, window, cx| v.show_view(View::Mentions, window, cx));
    settle(cx);
    // A response whose source this Mac doesn't hold, from a blyg not
    // subscribed: Subscribe opens with its address.
    let m = view.read_with(cx, |v, _| {
        v.reading
            .mentions
            .ready()
            .unwrap()
            .iter()
            .find(|m| {
                m.source_origin
                    .as_deref()
                    .is_some_and(|o| !v.backend.subscriptions().iter().any(|s| s.origin == o))
            })
            .cloned()
    });
    if let Some(m) = m {
        view.update_in(cx, |v, window, cx| v.reply_to_mention(&m, window, cx));
        settle(cx);
        let open = view.read_with(cx, |v, cx| match &v.reading.sheet {
            Some(super::super::RSheet::Subscribe { input, .. }) => {
                Some(input.read(cx).value().to_string())
            }
            _ => None,
        });
        assert_eq!(open, m.source_origin);
    }
    let _ = fake;
}

#[gpui_kit::test]
fn the_subscriptions_screen_shows_each_poll(cx: &mut TestAppContext) {
    let (view, _, cx) = setup(cx);
    view.update_in(cx, |v, window, cx| {
        v.show_view(View::Subscriptions, window, cx)
    });
    settle(cx);
    let lines: Vec<(String, bool)> = view.read_with(cx, |v, _| {
        v.reading
            .subs
            .iter()
            .map(|s| super::poll::status_line(s, v.now))
            .collect()
    });
    assert!(lines.iter().any(|(l, bad)| *bad
        && l.starts_with("3 polls failed in a row · last polled ")
        && l.ends_with("fetch error: the feed answered 502")));
    assert!(
        lines
            .iter()
            .filter(|(_, bad)| !bad)
            .all(|(l, _)| l.contains("last polled "))
    );
}

#[gpui_kit::test]
fn the_preview_renders_a_handle_as_the_contacts_h_card(cx: &mut TestAppContext) {
    let (_, fake, _) = setup(cx);
    let plain = crate::app::studio::resolver::StoreResolver::snapshot(&*fake, "/blyg");
    let opts = blyg_render::RenderOpts {
        data_line: false,
        ..Default::default()
    };
    let out = blyg_render::render_preview("Hi @lin.", blyg_render::Kind::Fragment, &plain, &opts);
    assert_eq!(
        out.html, "<p>Hi @lin.</p>\n",
        "no h-card where publish keeps the handle"
    );
    fake.set_rewrites_handles(true);
    let resolver = crate::app::studio::resolver::StoreResolver::snapshot(&*fake, "/blyg");
    let out = blyg_render::render_preview(
        "Tea with @lin and @sam.",
        blyg_render::Kind::Fragment,
        &resolver,
        &blyg_render::RenderOpts {
            data_line: false,
            ..Default::default()
        },
    );
    assert_eq!(
        out.html,
        format!(
            "<p>Tea with <span class=\"h-card\"><a class=\"u-url\" href=\"{LIN}\">@<span \
             class=\"p-name\">Lin</span></a><data class=\"u-url\" \
             value=\"https://social.example.net/@lin\"></data></span> and @sam.</p>\n"
        )
    );
    let _ = Kind::Fragment;
}
