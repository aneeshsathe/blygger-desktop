//! Bold, italic and formulas in the editor (studio `format-shortcuts.ts`):
//! markdown's markers written into the text as if typed, so undo takes them
//! back. ⌘B is the Shelf here, so bold and italic are ⌃⌘B and ⌃⌘I, and a
//! formula (the studio's Σ button) is ⌃⌘M; on Windows Alt+B, Alt+I and
//! Alt+M (`keymap::WINDOWS_OVERRIDES`).

use super::*;

gpui_kit::actions!(blygger, [FormatBold, FormatItalic, InsertFormula]);

/// Windows' one-time hint (`bold_key_hint`).
pub(crate) const BOLD_KEY_HINT: &str = "Bold is Alt+B; Ctrl+B opens the Shelf";

/// Replace `from..to` with `insert`, then select `start..end` (byte offsets).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FormatEdit {
    pub from: usize,
    pub to: usize,
    pub insert: String,
    pub start: usize,
    pub end: usize,
}

impl FormatEdit {
    /// The whole text after the edit.
    pub fn apply(&self, text: &str) -> String {
        format!("{}{}{}", &text[..self.from], self.insert, &text[self.to..])
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Emphasis {
    Bold,
    Italic,
}

impl Emphasis {
    fn width(self) -> usize {
        match self {
            Emphasis::Bold => 2,
            Emphasis::Italic => 1,
        }
    }

    /// Asterisks in a run: 1 italic, 2 bold, 3 both.
    fn is_on(self, run: usize) -> bool {
        match self {
            Emphasis::Bold => run >= 2,
            Emphasis::Italic => run % 2 == 1,
        }
    }
}

fn stars(s: &str) -> usize {
    s.bytes().take_while(|b| *b == b'*').count()
}

fn end_stars(s: &str) -> usize {
    s.bytes().rev().take_while(|b| *b == b'*').count()
}

fn wrap_line(line: &str, kind: Emphasis, on: bool) -> String {
    let w = kind.width();
    let core = line.trim();
    let lead = &line[..line.len() - line.trim_start().len()];
    let tail = &line[line.trim_end().len()..];
    let mark = "*".repeat(w);
    let middle = if on {
        format!("{mark}{core}{mark}")
    } else {
        core.get(w..core.len().saturating_sub(w))
            .unwrap_or_default()
            .to_string()
    };
    format!("{lead}{middle}{tail}")
}

/// `emphasis`: bold or italic on the selection `from..to` of `text`, or off
/// when it's already on (markers selected, or just outside the selection).
pub fn emphasis(text: &str, from: usize, to: usize, kind: Emphasis) -> FormatEdit {
    let picked = &text[from..to];
    let w = kind.width();
    // Across lines each line is wrapped on its own: emphasis can't span a
    // paragraph.
    if picked.contains('\n') {
        let lines: Vec<&str> = picked.split('\n').collect();
        let filled: Vec<&&str> = lines.iter().filter(|l| !l.trim().is_empty()).collect();
        let all_on = !filled.is_empty()
            && filled.iter().all(|l| {
                let core = l.trim();
                kind.is_on(stars(core).min(end_stars(core)))
            });
        let insert = lines
            .iter()
            .map(|l| {
                if l.trim().is_empty() {
                    l.to_string()
                } else {
                    wrap_line(l, kind, !all_on)
                }
            })
            .collect::<Vec<_>>()
            .join("\n");
        let end = from + insert.len();
        return FormatEdit {
            from,
            to,
            insert,
            start: from,
            end,
        };
    }
    // Spaces stay outside the markers, or markdown doesn't read emphasis.
    let lead = picked.len() - picked.trim_start().len();
    let tail = picked.len() - picked.trim_end().len();
    let blank = picked.trim().is_empty();
    let a = if blank { from } else { from + lead };
    let b = if blank { to } else { to - tail };
    let inner = &text[a..b];
    // Markers selected along with the words.
    let inner_run = stars(inner).min(end_stars(inner));
    if !inner.is_empty() && inner.len() > inner_run * 2 && kind.is_on(inner_run) {
        let bare = inner[w..inner.len() - w].to_string();
        let end = a + bare.len();
        return FormatEdit {
            from: a,
            to: b,
            insert: bare,
            start: a,
            end,
        };
    }
    // Markers just outside the selection, or around the caret.
    let run = end_stars(&text[..a]).min(stars(&text[b..]));
    if kind.is_on(run) {
        return FormatEdit {
            from: a - w,
            to: b + w,
            insert: inner.to_string(),
            start: a - w,
            end: a - w + inner.len(),
        };
    }
    let mark = "*".repeat(w);
    FormatEdit {
        from: a,
        to: b,
        insert: format!("{mark}{inner}{mark}"),
        start: a + w,
        end: a + w + inner.len(),
    }
}

/// `formulaEdit`: the selection between dollars, or an empty pair with the
/// caret inside. A selection over a line break is displayed (`$$…$$`); a
/// word or two is inline.
pub fn formula_edit(text: &str, from: usize, to: usize) -> FormatEdit {
    let picked = &text[from..to];
    let mark = if picked.contains('\n') { "$$" } else { "$" };
    let inner = picked.trim();
    let start = from + mark.len();
    FormatEdit {
        from,
        to,
        insert: format!("{mark}{inner}{mark}"),
        start,
        end: start + inner.len(),
    }
}

impl MainView {
    /// Hook: the format actions on the main window.
    pub(super) fn format_actions(&self, d: Stateful<Div>, cx: &mut Context<Self>) -> Stateful<Div> {
        d.on_action(cx.listener(|this, _: &FormatBold, window, cx| {
            this.format_edit(|t, a, b| emphasis(t, a, b, Emphasis::Bold), window, cx)
        }))
        .on_action(cx.listener(|this, _: &FormatItalic, window, cx| {
            this.format_edit(|t, a, b| emphasis(t, a, b, Emphasis::Italic), window, cx)
        }))
        .on_action(cx.listener(|this, _: &InsertFormula, window, cx| {
            this.format_edit(formula_edit, window, cx)
        }))
    }

    /// Windows writers press Ctrl+B for bold, but Ctrl+B is the Shelf there
    /// (bold is Alt+B). The first time it's pressed while writing, say so;
    /// once ever (`state.json`). User testing, R9.
    pub(crate) fn bold_key_hint(&mut self, window: &Window, cx: &mut Context<Self>) {
        use crate::platform_text::Platform;
        if Platform::spelling() != Platform::Windows
            || self.studio.guards.bold_hint_shown
            || self.current.is_none()
            || !self.editor.read(cx).focus_handle(cx).is_focused(window)
        {
            return;
        }
        self.studio.guards.bold_hint_shown = true;
        if let Some(dir) = super::command_palette::data_dir(cx) {
            let _ = blyg_core::state::AppState::update(&dir, |s| s.bold_key_hint_shown = true);
        }
        self.show_toast(BOLD_KEY_HINT, None, cx);
    }

    /// Apply an edit to the editor's selection, if the editor has the
    /// keyboard and a post is open.
    fn format_edit(
        &mut self,
        edit: impl Fn(&str, usize, usize) -> FormatEdit,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.current.is_none() || !self.editor.read(cx).focus_handle(cx).is_focused(window) {
            self.show_toast("Put the cursor in the post you're writing first", None, cx);
            return;
        }
        let (text, sel) = {
            let s = self.editor.read(cx);
            (s.value().to_string(), s.selected_range())
        };
        let (from, to) = (sel.start.min(text.len()), sel.end.min(text.len()));
        if !text.is_char_boundary(from) || !text.is_char_boundary(to) {
            return;
        }
        let e = edit(&text, from, to);
        let new = e.apply(&text);
        self.splice_editor(&text, &new, Some(e.start), window, cx);
        self.editor
            .update(cx, |s, cx| s.set_selected_range(e.start..e.end, cx));
    }
}

#[cfg(test)]
mod tests {
    use super::{Emphasis, emphasis, formula_edit};

    fn run(text: &str, from: usize, to: usize, kind: Emphasis) -> (String, String) {
        let e = emphasis(text, from, to, kind);
        let out = e.apply(text);
        let sel = out[e.start..e.end].to_string();
        (out, sel)
    }

    #[test]
    fn bold_and_italic_toggle() {
        assert_eq!(
            run("a word b", 2, 6, Emphasis::Bold),
            ("a **word** b".into(), "word".into())
        );
        assert_eq!(
            run("a **word** b", 4, 8, Emphasis::Bold),
            ("a word b".into(), "word".into()),
            "markers just outside: off"
        );
        assert_eq!(
            run("a **word** b", 2, 10, Emphasis::Bold),
            ("a word b".into(), "word".into()),
            "markers selected: off"
        );
        assert_eq!(
            run("a word b", 1, 7, Emphasis::Italic),
            ("a *word* b".into(), "word".into()),
            "spaces stay outside"
        );
        assert_eq!(
            run("ab", 1, 1, Emphasis::Italic),
            ("a**b".into(), "".into())
        );
        assert_eq!(
            run("one\n\ntwo", 0, 8, Emphasis::Bold).0,
            "**one**\n\n**two**"
        );
        assert_eq!(run("**one**\n**two**", 0, 15, Emphasis::Bold).0, "one\ntwo");
    }

    #[test]
    fn formulas() {
        let e = formula_edit("area x^2 here", 5, 8);
        assert_eq!(e.apply("area x^2 here"), "area $x^2$ here");
        assert_eq!((e.start, e.end), (6, 9));
        let e = formula_edit("", 0, 0);
        assert_eq!((e.insert.as_str(), e.start, e.end), ("$$", 1, 1));
        // A whole line picked with its line break: `$$` on lines of their
        // own, the break kept outside (user testing, R4).
        let t = "a\n\\int f\n";
        let e = formula_edit(t, 2, 9);
        assert_eq!(e.apply(t), "a\n$$\n\\int f\n$$\n");
        assert_eq!(&e.apply(t)[e.start..e.end], "\\int f");
    }

    /// User testing (R4): ⇧↓ over the middle line, then ⌃⌘M.
    #[test]
    fn a_formula_over_a_whole_line_keeps_the_text_after_it() {
        let t = "Area:\n\\frac{a}{b}\nand on.";
        let from = t.find('\\').unwrap();
        let to = t.find("and").unwrap();
        let e = formula_edit(t, from, to);
        let out = e.apply(t);
        assert_eq!(out, "Area:\n$$\n\\frac{a}{b}\n$$\nand on.");
        assert_eq!(&out[e.start..e.end], "\\frac{a}{b}");
        // It draws as displayed maths.
        let html = blyg_render::render_markdown(&out);
        assert!(html.contains("display=\"block\""), "{html}");
        assert!(html.contains("and on."), "{html}");
        // Picked mid-line, across a break: the `$$` still go on their own.
        let t = "say a\nb now";
        let e = formula_edit(t, 4, 7);
        assert_eq!(e.apply(t), "say \n$$\na\nb\n$$\n now");
    }
}
