//! --- capture --- A web address pasted into the omnibar (and only the
//! omnibar): ⏎ starts a fragment with that link (proposal 12). The page's
//! title is fetched once, on ⏎, with the token-less public client
//! (`Backend::page_title`); without one the address is the link's text.
//! An address that names a blyg post held here is quoted instead,
//! `![[id]]` in a thread, as the notes drawer does.

use blyg_core::Kind;
use gpui_kit::*;

use crate::app::MainView;
use crate::app::reading::vm as rvm;

/// The new note's kind and text for `url`: a held blyg post's id quotes
/// it; otherwise a fragment holding the link, titled when there's a title.
pub(crate) fn link_note(url: &str, held_id: Option<&str>, title: Option<&str>) -> (Kind, String) {
    match held_id {
        Some(id) => (Kind::Thread, format!("![[{id}]]\n")),
        None => (
            Kind::Fragment,
            crate::app::notes::link(title.unwrap_or(""), url),
        ),
    }
}

impl MainView {
    /// ⏎ on a lone web address with no matches.
    pub(crate) fn capture_link(
        &mut self,
        url: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let held = rvm::blyg_item_for_url(&url, &self.reading.rows, &self.reading.subs)
            .map(|r| r.remote_id.clone());
        let backend = self.backend.clone();
        let task = cx.background_spawn(async move {
            let title = match held {
                Some(_) => None,
                None => backend.page_title(&url),
            };
            link_note(&url, held.as_deref(), title.as_deref())
        });
        cx.spawn_in(window, async move |this, cx| {
            let (kind, text) = task.await;
            let _ = this.update_in(cx, |v, window, cx| {
                v.create_from(&text, window, cx);
                if kind == Kind::Thread
                    && let Some(id) = v.current.as_ref().map(|c| c.local_id.clone())
                {
                    let _ = v.backend.set_kind(&id, kind);
                    v.requery(window, cx);
                }
            });
        })
        .detach();
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use blyg_core::{Backend, ConfigStore, Kind};
    use gpui_kit::{Entity, TestAppContext, VisualTestContext};

    use super::link_note;
    use crate::app::MainView;
    use crate::fake::{FakeBackend, Timing};
    use crate::prefs::Prefs;

    #[test]
    fn a_link_note_is_titled_or_quotes_a_held_post() {
        assert_eq!(
            link_note(
                "https://lin.example.org/t/x",
                None,
                Some("Rock pool economics")
            ),
            (
                Kind::Fragment,
                "[Rock pool economics](https://lin.example.org/t/x)".into()
            )
        );
        assert_eq!(
            link_note("https://a.example.com/", None, None).1,
            "[https://a.example.com/](https://a.example.com/)"
        );
        assert_eq!(
            link_note(
                "https://lin.example.org/t/x",
                Some("01jxn"),
                Some("ignored")
            ),
            (Kind::Thread, "![[01jxn]]\n".into())
        );
    }

    fn setup(
        cx: &mut TestAppContext,
    ) -> (Entity<MainView>, Arc<FakeBackend>, &mut VisualTestContext) {
        let config = "blyg-url = https://blyg.example.com\nnew-note = draft\n";
        let prefs = Prefs::from_config(ConfigStore::in_memory(config).config());
        cx.update(|cx| {
            gpui_kit::init(cx);
            crate::app::bind_keys(cx);
            crate::settings::init(
                ConfigStore::in_memory(config),
                Arc::new(blyg_core::config::MemoryTokenStore::default()),
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

    #[gpui_kit::test]
    fn a_pasted_address_starts_a_fragment_with_its_title(cx: &mut TestAppContext) {
        let (view, fake, cx) = setup(cx);
        let before = fake.items().len();
        cx.simulate_input("https://lin.example.org/t/rock-pools");
        cx.run_until_parked();
        view.read_with(cx, |v, _| {
            assert_eq!(v.list.hint(), "⏎ start a fragment with this link");
        });
        cx.simulate_keystrokes("enter");
        cx.run_until_parked();
        let items = fake.items();
        assert_eq!(items.len(), before + 1);
        let new = items
            .iter()
            .find(|i| i.content_md.contains("rock-pools"))
            .expect("a new note");
        assert_eq!(new.kind, Kind::Fragment);
        assert_eq!(
            new.content_md,
            "[Rock pool economics](https://lin.example.org/t/rock-pools)"
        );
    }

    #[gpui_kit::test]
    fn an_address_without_a_title_links_itself(cx: &mut TestAppContext) {
        let (_view, fake, cx) = setup(cx);
        cx.simulate_input("https://elsewhere.example.net/page");
        cx.run_until_parked();
        cx.simulate_keystrokes("enter");
        cx.run_until_parked();
        assert!(fake.items().iter().any(|i| i.content_md
            == "[https://elsewhere.example.net/page](https://elsewhere.example.net/page)"));
    }
}
