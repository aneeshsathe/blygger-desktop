//! Main-window sheets. At most one is open (`MainView::sheet`, a
//! [`SheetSlot`]). Each kind is a struct that owns its state and implements
//! [`SheetView`]: how it draws, what its keys do (`esc`, `⏎` and the rest
//! are in its render's key handlers), which text fields it has, and what
//! ⌘⏎ does while it is up. The sheets that belong to a feature live with
//! it (`shelf.rs`, `contacts/`, `upkeep/`, …); the core ones live here.
//!
//! The registry below is the one list of kinds, and its `view` is the one
//! `match` on them. The enum is private to this module, so code elsewhere
//! can't match a variant: it asks the slot (`is::<T>()`, `get::<T>()`,
//! `take::<T>()`, `open(..)`, `inputs()`, …) instead.

use gpui_kit::base::input::InputState;
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;

use super::MainView;
use crate::theme::{Palette, Theme};

pub(crate) mod conflict;
pub(crate) mod connect;
pub(crate) mod publish;

/// Declares the kinds: the private enum (one boxed variant per kind), the
/// slot's dispatch `view`, and each kind's typed access ([`SheetKind`]).
macro_rules! sheets {
    ($($(#[$m:meta])* $name:ident($ty:ty),)*) => {
        enum Sheet {
            $($(#[$m])* $name(Box<$ty>),)*
        }

        impl Sheet {
            /// The one dispatch on the kinds.
            fn view(&self) -> &dyn SheetView {
                match self {
                    $(Sheet::$name(s) => &**s,)*
                }
            }
        }

        $(
            impl SheetKind for $ty {
                fn peek(slot: &SheetSlot) -> Option<&Self> {
                    match &slot.open {
                        Some(Sheet::$name(s)) => Some(s),
                        _ => None,
                    }
                }
                fn peek_mut(slot: &mut SheetSlot) -> Option<&mut Self> {
                    match &mut slot.open {
                        Some(Sheet::$name(s)) => Some(s),
                        _ => None,
                    }
                }
                fn take_from(slot: &mut SheetSlot) -> Option<Self> {
                    match slot.open.take() {
                        Some(Sheet::$name(s)) => Some(*s),
                        _ => None,
                    }
                }
                fn put_in(self, slot: &mut SheetSlot) {
                    slot.open = Some(Sheet::$name(Box::new(self)));
                }
            }
        )*
    };
}

sheets! {
    Publish(publish::PublishSheet),
    /// Publishing labelled links to a blyg below studio 0.47.
    HeldLabels(publish::HeldLabelsSheet),
    /// --- tangle --- a thread's spans would publish new items: confirm.
    Tangle(publish::TangleSheet),
    Conflict(conflict::ConflictSheet),
    /// Not a sheet: the Settings page over the whole window.
    Settings(super::settings_page::SettingsSheet),
    Connect(connect::ConnectSheet),
    Disconnect(connect::DisconnectSheet),
    // --- delete & withdraw --- (discard.rs)
    DeleteDraft(super::discard::DeleteDraftSheet),
    Withdraw(super::discard::WithdrawSheet),
    DiscardChanges(super::discard::DiscardChangesSheet),
    // --- browser macros --- (browser/automation)
    MacroPreview(super::browser::automation::PreviewSheet),
    MacroPost(super::browser::automation::PostSheet),
    // --- blyg switcher ---
    Blygs(super::switcher::BlygSheet),
    // --- shelf ---
    Shelf(super::shelf::ShelfSheet),
    // --- access-ai ---
    ClientAccess(super::access::ClientAccessSheet),
    ServerAi(super::server_ai::ServerAiSheet),
    Dictionary(super::server_ai::dictionary::DictionarySheet), // --- dictionary ---
    // --- contacts ---
    Contacts(super::reading::contacts::ContactsSheet),
    // --- upkeep ---
    StaleQuotes(super::upkeep::StaleSheet),
    Signals(super::upkeep::SignalsSheet),
    AltText(super::upkeep::AltSheet),
    // --- syndication ---
    Copies(super::syndication::CopiesSheet),
}

/// What a sheet draws.
pub(crate) enum Shown {
    /// A sheet of this width under the title bar.
    Sheet(f32, AnyElement),
    /// A page over the whole window (Settings).
    Page(AnyElement),
}

impl From<(f32, AnyElement)> for Shown {
    fn from((w, el): (f32, AnyElement)) -> Self {
        Shown::Sheet(w, el)
    }
}

/// ⌘⏎ (the Publish action) while a sheet is up.
pub(crate) type Submit = fn(&mut MainView, &mut Window, &mut Context<MainView>);

/// One kind of sheet's behaviour.
pub(crate) trait SheetView {
    /// The sheet's content, with its key handlers (`esc`, `⏎`, …).
    fn render(&self, v: &MainView, kit: &Kit, cx: &mut Context<MainView>) -> Shown;
    /// Its text fields, which follow the theme's field style.
    fn inputs(&self) -> Vec<&Entity<InputState>> {
        Vec::new()
    }
    /// What ⌘⏎ does; `None`: nothing (it never publishes under a sheet).
    fn submit(&self) -> Option<Submit> {
        None
    }
}

/// Typed access to a kind in the slot. Written by `sheets!` for each kind
/// in the registry; call it through [`SheetSlot`].
pub(crate) trait SheetKind: SheetView + Sized {
    fn peek(slot: &SheetSlot) -> Option<&Self>;
    fn peek_mut(slot: &mut SheetSlot) -> Option<&mut Self>;
    fn take_from(slot: &mut SheetSlot) -> Option<Self>;
    fn put_in(self, slot: &mut SheetSlot);
}

/// The main window's sheet: none, or one of the registry's kinds.
#[derive(Default)]
pub(crate) struct SheetSlot {
    open: Option<Sheet>,
    /// Counts opens: keys the slide-in animation, and tells a reply for an
    /// earlier sheet from one for this one.
    generation: usize,
}

impl SheetSlot {
    /// A sheet (or the Settings page) is up.
    pub(crate) fn is_open(&self) -> bool {
        self.open.is_some()
    }

    /// The open sheet is a `T`.
    pub(crate) fn is<T: SheetKind>(&self) -> bool {
        T::peek(self).is_some()
    }

    pub(crate) fn get<T: SheetKind>(&self) -> Option<&T> {
        T::peek(self)
    }

    pub(crate) fn get_mut<T: SheetKind>(&mut self) -> Option<&mut T> {
        T::peek_mut(self)
    }

    /// Closes whatever is open, and hands it back if it was a `T`.
    pub(crate) fn take<T: SheetKind>(&mut self) -> Option<T> {
        T::take_from(self)
    }

    /// Opens `sheet` in place of any other.
    pub(crate) fn open<T: SheetKind>(&mut self, sheet: T) {
        self.generation += 1;
        sheet.put_in(self);
    }

    /// Closes the open sheet. `MainView::close_sheet` also puts focus back.
    pub(crate) fn close(&mut self) {
        self.open = None;
    }

    /// Bumped by every `open`.
    pub(crate) fn generation(&self) -> usize {
        self.generation
    }

    /// The open sheet's text fields.
    pub(crate) fn inputs(&self) -> Vec<&Entity<InputState>> {
        self.open
            .as_ref()
            .map_or_else(Vec::new, |s| s.view().inputs())
    }

    /// What ⌘⏎ does under the open sheet (`None` also when none is open).
    pub(crate) fn submit(&self) -> Option<Submit> {
        self.open.as_ref().and_then(|s| s.view().submit())
    }
}

/// The pieces sheets share: keys drawn as caps, hints, headings, fields.
pub(crate) struct Kit<'a> {
    pub p: Palette,
    pub theme: &'a Theme,
    pub ui_font: &'a SharedString,
    pub body_font: &'a SharedString,
}

impl Kit<'_> {
    pub(crate) fn kbd(&self, k: &'static str) -> Div {
        crate::theme_ext::kbd(div(), self.theme)
            .px(px(6.))
            .py(px(1.))
            .min_w(px(20.))
            .flex()
            .justify_center()
            .font_weight(FontWeight::MEDIUM)
            .text_size(px(11.5))
            .child(k)
    }

    pub(crate) fn key_hint(&self, k: &'static str, label: &'static str) -> Div {
        div()
            .flex()
            .items_center()
            .gap(px(5.))
            .child(self.kbd(k))
            .child(label)
    }

    pub(crate) fn keys_row(&self, items: Vec<Div>) -> Div {
        div()
            .mt(px(12.))
            .flex()
            .gap(px(14.))
            .text_color(self.p.muted)
            .children(items)
    }

    pub(crate) fn heading(&self, s: String) -> Div {
        div()
            .mb(px(8.))
            .font_weight(FontWeight::SEMIBOLD)
            .text_size(px(13.))
            .child(s)
    }

    pub(crate) fn input_box(&self, el: AnyElement) -> Div {
        crate::theme_ext::field_box(div(), self.theme)
            .px(px(10.))
            .py(px(8.))
            .text_size(px(14.))
            .font_family(self.ui_font.clone())
            .child(el)
    }
}

impl MainView {
    /// The open sheet in its frame (or the Settings page), if one is up.
    pub(super) fn render_sheet(
        &self,
        ui_font: &SharedString,
        body_font: &SharedString,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let sheet = self.sheet.open.as_ref()?;
        let kit = Kit {
            p: self.palette,
            theme: &self.theme,
            ui_font,
            body_font,
        };
        let (width, content) = match sheet.view().render(self, &kit, cx) {
            Shown::Sheet(width, content) => (width, content),
            Shown::Page(page) => return Some(page),
        };

        let generation = self.sheet.generation;
        Some(
            div()
                .absolute()
                .top(px(super::TITLEBAR_H))
                .left_0()
                // --- browser macros --- beside the side pane, not over it
                .right(self.macro_sheet_right().unwrap_or(px(0.)))
                .flex()
                .justify_center()
                .child(
                    div()
                        .id("sheet")
                        .occlude()
                        .w(px(width))
                        .max_w(relative(0.92))
                        .map(|d| crate::theme_ext::sheet(d, &self.theme))
                        .px(px(18.))
                        .py(px(16.))
                        .font_family(ui_font.clone())
                        .text_size(px(13.))
                        .child(content)
                        .with_animation(
                            ("sheet-in", generation),
                            Animation::new(std::time::Duration::from_millis(220))
                                .with_easing(ease_out_quint()),
                            |d, t| d.mt(px(-240.0 * (1.0 - t))).opacity(t.min(1.0) * 0.4 + 0.6),
                        ),
                )
                .into_any_element(),
        )
    }
}
