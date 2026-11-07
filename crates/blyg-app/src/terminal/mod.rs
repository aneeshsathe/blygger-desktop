//! --- terminal --- The experimental terminal pane: the user's login shell
//! at the foot of the window, for a CLI agent (claude, codex…), git or
//! `wrangler dev` beside the posts.
//!
//! - ⌃` (Ctrl+` on Windows; View › Terminal) shows the pane and gives it the
//!   keyboard; ⌃` again (with the keyboard in it) hides it. Hiding keeps the
//!   shell running; the header's "end" button, `exit`, or `terminal = false`
//!   end it. It's a layout row like the reading compose box, between the
//!   body and the status bar, never over the editor.
//! - ⇧⌘T (Post › Send Selection to Terminal) pastes the editor's selection
//!   into the shell as a Markdown quote, for the agent to read.
//! - The engine and the pty are `blyg-term` (Alacritty's terminal core and
//!   portable-pty; libghostty-vt is a cargo feature there, off). The pane
//!   draws the cells itself (`view.rs`): one shaped run per row segment, quads
//!   for backgrounds, the cursor and the selection, the theme's colours.
//! - Safety: the shell gets a scrubbed environment (`blyg_term::env`), Burrow
//!   never puts a token in it, in a file or on the clipboard for it, and the
//!   pane has no IPC bridge into the app: Burrow talks to the shell only
//!   through the pty.
//!
//! A child module of `app` (it reads `MainView`); `app.rs` keeps small hooks
//! marked `// --- terminal ---`.

mod view;

#[cfg(test)]
mod tests;

use super::*;

pub use view::{TermEvent, TerminalView};

gpui_kit::actions!(blygger, [ToggleTerminal, SendSelectionToTerminal]);

/// The pane is experimental (the header and the menu say so).
pub const EXPERIMENTAL: bool = true;

/// The pane's state on the main window.
#[derive(Default)]
pub struct Dock {
    /// The pane is on screen.
    pub open: bool,
    /// The running session (kept while the pane is hidden).
    pub view: Option<Entity<TerminalView>>,
    /// Where the keyboard was before the pane took it.
    restore: Option<FocusHandle>,
    _sub: Option<Subscription>,
    /// Tests: start this instead of the login shell (`None`: no shell, the
    /// tests feed the engine).
    #[cfg(test)]
    pub test_shell: Option<blyg_term::pty::ShellSpec>,
}

/// The config's terminal keys.
fn config(cx: &App) -> (bool, Option<String>) {
    match cx.try_global::<crate::settings::AppConfig>() {
        Some(a) => {
            let c = a.store.config();
            (c.terminal(), c.terminal_shell().map(str::to_string))
        }
        None => (true, None),
    }
}

impl MainView {
    /// Hook: the pane's actions on the main window.
    pub(super) fn terminal_actions(
        &self,
        d: Stateful<Div>,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        d.on_action(
            cx.listener(|this, _: &ToggleTerminal, window, cx| this.toggle_terminal(window, cx)),
        )
        .on_action(
            cx.listener(|this, _: &SendSelectionToTerminal, window, cx| {
                this.send_selection_to_terminal(window, cx)
            }),
        )
    }

    /// ⌃`: show the pane (starting the shell the first time) and give it
    /// the keyboard; with the keyboard already in it, hide it.
    pub(crate) fn toggle_terminal(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !config(cx).0 {
            self.show_toast(
                "The terminal is off",
                Some("terminal = true in the config turns it on".into()),
                cx,
            );
            return;
        }
        let focused = self
            .terminal
            .view
            .as_ref()
            .is_some_and(|v| v.read(cx).focus.is_focused(window));
        if self.terminal.open && focused {
            self.hide_terminal(window, cx);
            return;
        }
        if !self.terminal.open {
            self.terminal.restore = window.focused(cx);
        }
        let view = self.ensure_terminal(window, cx);
        self.terminal.open = true;
        let focus = view.read(cx).focus.clone();
        window.focus(&focus, cx);
        cx.notify();
    }

    /// The running session, started if there's none.
    fn ensure_terminal(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Entity<TerminalView> {
        if let Some(v) = &self.terminal.view {
            return v.clone();
        }
        let spec = self.terminal_shell_spec(cx);
        let palette = self.palette;
        let view = cx.new(|cx| TerminalView::new(spec, palette, window, cx));
        self.terminal._sub = Some(cx.subscribe_in(
            &view,
            window,
            |this, _, e: &TermEvent, window, cx| match e {
                TermEvent::Exited => {
                    this.end_terminal(window, cx);
                    this.show_toast("The shell exited", None, cx);
                }
                TermEvent::Hide => this.hide_terminal(window, cx),
                TermEvent::End => this.end_terminal(window, cx),
            },
        ));
        self.terminal.view = Some(view.clone());
        view
    }

    /// What the pane runs: `terminal-shell`, else the login shell. In the
    /// app's own tests, nothing unless a test asks.
    fn terminal_shell_spec(&self, cx: &App) -> Option<blyg_term::pty::ShellSpec> {
        #[cfg(test)]
        {
            let _ = cx;
            self.terminal.test_shell.clone()
        }
        #[cfg(not(test))]
        {
            Some(blyg_term::pty::ShellSpec::login(config(cx).1.as_deref()))
        }
    }

    /// Hide the pane; the shell keeps running.
    pub(crate) fn hide_terminal(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.terminal.open {
            return;
        }
        self.terminal.open = false;
        match self.terminal.restore.take() {
            Some(f) => window.focus(&f, cx),
            None => window.focus(&self.focus, cx),
        }
        cx.notify();
    }

    /// End the shell and close the pane.
    pub(crate) fn end_terminal(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.hide_terminal(window, cx);
        self.terminal._sub = None;
        // Dropping the view drops its pty, which kills the shell it started.
        self.terminal.view = None;
        cx.notify();
    }

    /// ⇧⌘T: the editor's selection, quoted, into the shell (not run: the
    /// user adds their question and presses ⏎).
    pub(crate) fn send_selection_to_terminal(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let (text, range) = {
            let e = self.editor.read(cx);
            (e.value().to_string(), e.selected_range())
        };
        let selected = text
            .get(range.start.min(range.end)..range.end.max(range.start))
            .unwrap_or("")
            .trim();
        if selected.is_empty() {
            self.show_toast("Select some text in the editor first", None, cx);
            return;
        }
        let quoted = format!("{}\n\n", blyg_term::keys::quote(selected));
        if !(self.terminal.open
            && self
                .terminal
                .view
                .as_ref()
                .is_some_and(|v| v.read(cx).focus.is_focused(window)))
        {
            self.toggle_terminal(window, cx);
        }
        if let Some(v) = &self.terminal.view {
            v.update(cx, |t, cx| t.paste(&quoted, cx));
        }
    }

    /// Hook: the pane, when it's open, as a row above the status bar.
    pub(super) fn render_terminal(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        if !self.terminal.open {
            return None;
        }
        let view = self.terminal.view.clone()?;
        let p = self.palette;
        let chrome: SharedString = self.theme.chrome_font(&self.prefs).into();
        view.update(cx, |t, _| t.set_style(p, chrome));
        let h = (window.viewport_size().height * 0.34)
            .round()
            .clamp(px(140.), px(460.));
        Some(
            div()
                .id("terminal-pane")
                .flex_none()
                .h(h)
                .w_full()
                .rule_t(&p)
                .child(view)
                .into_any_element(),
        )
    }

    /// Hook: after a config reload, `terminal = false` ends the shell.
    pub(super) fn terminal_reload(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !config(cx).0 && self.terminal.view.is_some() {
            self.end_terminal(window, cx);
        }
    }

    /// Hook (tour): leave the next step a clear window; the shell keeps
    /// running.
    pub(crate) fn terminal_tour_tidy(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.hide_terminal(window, cx);
    }
}
