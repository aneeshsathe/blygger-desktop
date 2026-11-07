//! --- upkeep --- "Changes from vN" in ⌘Y (Studio's "see the change"): a
//! word diff of a version of your post against the version before it,
//! from the words the blyg keeps for each (`GET /items/{id}/versions/{v}`).
//! What came in is shaded green, what went is struck through.

use blyg_core::Version;

use super::*;

/// The diff shown in place of the version's text.
#[derive(Debug, Clone, PartialEq)]
pub struct VersionDiff {
    pub id: LocalId,
    pub from: u32,
    pub to: u32,
    pub runs: Fetch<vm::DiffRuns>,
}

/// The version `v` is compared with: the newest one before it that has
/// words (a withdrawal marker has none). `None` for the first.
pub fn previous(list: &[Version], v: u32) -> Option<u32> {
    list.iter()
        .filter(|x| x.version < v && !x.endcap)
        .map(|x| x.version)
        .max()
}

impl MainView {
    /// Hook (⌘Y's chips for the selected version): "Changes from vN", or
    /// "Hide changes" while they show.
    pub(crate) fn upkeep_diff_chip(
        &self,
        id: &LocalId,
        list: &[Version],
        v: u32,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let this_v = list.iter().find(|x| x.version == v)?;
        if this_v.endcap {
            return None;
        }
        let from = previous(list, v)?;
        let showing = self
            .upkeep
            .diff
            .as_ref()
            .is_some_and(|d| &d.id == id && d.to == v);
        let label = if showing {
            "Hide changes".to_string()
        } else {
            format!("Changes from v{from}")
        };
        let id = id.clone();
        Some(
            self.upkeep_chip("own-diff", label, true)
                .debug_selector(|| "own-diff".into())
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.toggle_version_diff(id.clone(), from, v, cx)
                }))
                .into_any_element(),
        )
    }

    pub(crate) fn toggle_version_diff(
        &mut self,
        id: LocalId,
        from: u32,
        to: u32,
        cx: &mut Context<Self>,
    ) {
        if self
            .upkeep
            .diff
            .as_ref()
            .is_some_and(|d| d.id == id && d.to == to)
        {
            self.upkeep.diff = None;
            return cx.notify();
        }
        self.upkeep.diff = Some(VersionDiff {
            id: id.clone(),
            from,
            to,
            runs: Fetch::Loading,
        });
        let backend = self.backend.clone();
        let id2 = id.clone();
        let task = cx.background_spawn(async move {
            let old = backend.version_text(&id2, from)?;
            let new = backend.version_text(&id2, to)?;
            Ok::<_, blyg_core::CoreError>(vm::word_diff(&old, &new).1)
        });
        cx.spawn(async move |this, cx| {
            let r = task.await;
            let _ = this.update(cx, |v, cx| {
                if let Some(d) = v.upkeep.diff.as_mut().filter(|d| d.id == id && d.to == to) {
                    d.runs = Fetch::of(r);
                    cx.notify();
                }
            });
        })
        .detach();
        cx.notify();
    }

    /// Hook (⌘Y's document): the diff, when it's shown for version `sel`.
    pub(crate) fn upkeep_diff_body(
        &self,
        id: &LocalId,
        sel: u32,
        body_font: &SharedString,
        size: f32,
    ) -> Option<AnyElement> {
        let d = self
            .upkeep
            .diff
            .as_ref()
            .filter(|d| &d.id == id && d.to == sel)?;
        let p = self.palette.on_page();
        let heading = div()
            .mb(px(8.))
            .font_family(self.chrome())
            .text_size(px(12.))
            .text_color(p.muted)
            .child(format!("What changed from v{} to v{}", d.from, d.to));
        let content: AnyElement = match &d.runs {
            Fetch::Loading => self.upkeep_note("Reading both versions…"),
            Fetch::Failed(e) => self.upkeep_note(format!("Couldn't read the versions: {e}")),
            Fetch::Ready(runs) if runs.iter().all(|(op, _)| *op == DiffOp::Same) => {
                self.upkeep_note("The same words: this version changed nothing in the text.")
            }
            Fetch::Ready(runs) => {
                let text: String = runs.iter().map(|(_, s)| s.as_str()).collect();
                let mut hl = Vec::new();
                let mut at = 0;
                for (op, s) in runs {
                    let r = at..at + s.len();
                    at += s.len();
                    match op {
                        DiffOp::Insert => hl.push((
                            r,
                            HighlightStyle {
                                background_color: Some(p.ins_bg),
                                ..Default::default()
                            },
                        )),
                        DiffOp::Delete => hl.push((
                            r,
                            HighlightStyle {
                                background_color: Some(p.del_bg),
                                strikethrough: Some(StrikethroughStyle {
                                    thickness: px(1.),
                                    color: Some(p.over),
                                }),
                                ..Default::default()
                            },
                        )),
                        DiffOp::Same => {}
                    }
                }
                div()
                    .debug_selector(|| "own-diff-text".into())
                    .font_family(body_font.clone())
                    .text_size(px(size))
                    .line_height(relative(1.6))
                    .child(StyledText::new(text).with_highlights(hl))
                    .into_any_element()
            }
        };
        Some(
            div()
                .id("own-diff-body")
                .flex_1()
                .min_h_0()
                .overflow_y_scroll()
                .px(px(32.))
                .pb(px(24.))
                .child(heading)
                .child(content)
                .into_any_element(),
        )
    }
}

#[cfg(test)]
mod unit_tests {
    use super::previous;
    use blyg_core::Version;

    fn v(version: u32, endcap: bool) -> Version {
        Version {
            version,
            published_at: String::new(),
            note: None,
            pinned: false,
            endcap,
        }
    }

    #[test]
    fn a_version_is_compared_with_the_last_one_that_had_words() {
        let list = [v(1, false), v(2, true), v(3, false)];
        assert_eq!(previous(&list, 3), Some(1));
        assert_eq!(previous(&list, 1), None);
    }
}
