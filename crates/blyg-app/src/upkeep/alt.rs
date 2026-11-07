//! --- upkeep --- Alt text for an image you paste or drop into a post.
//!
//! Once the upload lands and `![](url)` is in the text, a small sheet asks
//! what the image shows; ⏎ writes it as the image's alt text (`![a tide
//! pool at dusk](url)`), esc leaves it without. The words live in the post's
//! Markdown, where the blyg renders them as the image's `alt`: Burrow's
//! uploads are inline, never attachments, so the text is the one place a
//! reader's screen reader finds them.

use super::*;

/// The sheet.
pub(crate) struct AltSheet {
    pub id: LocalId,
    /// The image's address as written in the post.
    pub url: String,
    pub input: Entity<InputState>,
}

/// Alt text as Markdown needs it inside `![…]`: one line, brackets escaped.
pub fn clean_alt(alt: &str) -> String {
    let one_line = alt.split_whitespace().collect::<Vec<_>>().join(" ");
    vm::escape_link_text(&one_line)
}

/// `text` with the image at `url` given `alt`: its first `![](url)`.
/// `None` when that image isn't there with an empty alt (moved, deleted,
/// or described by hand meanwhile).
pub fn set_alt(text: &str, url: &str, alt: &str) -> Option<String> {
    let alt = clean_alt(alt);
    if alt.is_empty() {
        return None;
    }
    let bare = vm::image_markdown(url);
    let at = text.find(&bare)?;
    Some(format!(
        "{}![{alt}]({url}){}",
        &text[..at],
        &text[at + bare.len()..]
    ))
}

impl MainView {
    /// Hook (an upload just landed in a post): ask what the image shows,
    /// unless another sheet is up.
    pub(crate) fn upkeep_ask_alt(
        &mut self,
        id: LocalId,
        url: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.sheet.is_some() || self.current.as_ref().map(|c| &c.local_id) != Some(&id) {
            return;
        }
        let input = cx.new(|cx| {
            InputState::new(window, cx).placeholder("e.g. a tide pool at dusk, two crabs")
        });
        let sub = cx.subscribe_in(&input, window, |this, _, ev, window, cx| {
            if let InputEvent::PressEnter { .. } = ev {
                this.confirm_alt(window, cx);
            }
        });
        self._subs.push(sub);
        input.update(cx, |s, cx| s.focus(window, cx));
        self.sheet_gen += 1;
        self.sheet = Some(Sheet::AltText(AltSheet { id, url, input }));
        cx.notify();
    }

    /// ⏎: write the alt text into the post.
    pub(crate) fn confirm_alt(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(Sheet::AltText(s)) = &self.sheet else {
            return;
        };
        let alt = s.input.read(cx).value().to_string();
        let (id, url) = (s.id.clone(), s.url.clone());
        self.sheet = None;
        let editing = self.current.as_ref().is_some_and(|c| c.local_id == id);
        let text = if editing {
            self.editor.read(cx).value().to_string()
        } else {
            self.backend
                .item(&id)
                .map(|i| i.content_md)
                .unwrap_or_default()
        };
        match set_alt(&text, &url, &alt) {
            Some(new) if editing => self.splice_editor(&text, &new, None, window, cx),
            Some(new) => {
                let _ = self.backend.save(&id, &new);
            }
            None => {}
        }
        self.focus_after_sheet(window, cx);
        cx.notify();
    }

    pub(crate) fn render_alt_sheet(
        &self,
        s: &AltSheet,
        cx: &mut Context<Self>,
    ) -> (f32, AnyElement) {
        let p = self.palette;
        let body = div()
            .capture_action(cx.listener(|this, _: &Escape, window, cx| {
                cx.stop_propagation();
                this.close_sheet(window, cx);
            }))
            .child(
                div()
                    .mb(px(6.))
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_size(px(13.))
                    .child("Describe the image"),
            )
            .child(
                div()
                    .mb(px(8.))
                    .text_size(px(12.))
                    .line_height(relative(1.45))
                    .text_color(p.muted)
                    .child(
                        "Alt text: what a reader who can't see it should know. It goes into \
                         the post as the image's description.",
                    ),
            )
            .child(
                crate::theme_ext::field_box(div(), &self.theme)
                    .px(px(10.))
                    .py(px(7.))
                    .text_size(px(13.5))
                    .child(gpui_kit::base::input::Input::new(&s.input)),
            )
            .child(
                div()
                    .mt(px(10.))
                    .text_size(px(11.))
                    .text_color(p.muted)
                    .child("⏎ adds it · esc leaves the image without one"),
            );
        (440., body.into_any_element())
    }
}

#[cfg(test)]
mod unit_tests {
    use super::{clean_alt, set_alt};

    #[test]
    fn the_alt_goes_into_the_first_bare_image_of_that_address() {
        let url = "https://blyg.example.com/media/a.png";
        let text = format!("Tide pools.\n\n![]({url})\n\nMore.");
        assert_eq!(
            set_alt(&text, url, "  two crabs\n[at dusk] ").unwrap(),
            format!("Tide pools.\n\n![two crabs \\[at dusk\\]]({url})\n\nMore.")
        );
        assert_eq!(set_alt(&text, url, "   "), None, "nothing typed");
        assert_eq!(set_alt("no image", url, "x"), None);
        assert_eq!(clean_alt("a\nb"), "a b");
    }
}
