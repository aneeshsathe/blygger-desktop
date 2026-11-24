//! What blyg-app's `platform_text` gives the view-models this crate
//! compiles unchanged (`vm.rs`, `reading/vm.rs`): app text written in macOS
//! terms, read with [`Text::get`].
//!
//! The browser has one spelling of its own, [`Platform::Browser`]: the
//! Mac's glyphs with ⌃ where the Mac says ⌘, since the browser keeps ⌘N,
//! ⌘T and ⌘L and the page binds ⌃ (it takes ⌘ too where it can). Outside
//! tests [`Platform::spelling`] is always [`Platform::Browser`]. The Mac's
//! and Windows' spellings are here so the shared view-models' own tests,
//! which pin both platforms' wording (`spelled_as`, the Mac's by default),
//! run in this crate as they do in blyg-app.

/// The platform whose wording the text uses.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Platform {
    Mac,
    Windows,
    /// The page's own: ⌃ for ⌘ (`keymap::hint`).
    Browser,
}

impl Platform {
    /// The spelling the text uses: the browser's, on any system.
    #[inline]
    pub fn spelling() -> Platform {
        #[cfg(test)]
        {
            test_spelling::current()
        }
        #[cfg(not(test))]
        {
            Platform::Browser
        }
    }
}

/// Run `f` with the text spelled as `p` on this thread (tests only).
#[cfg(test)]
pub fn spelled_as<R>(p: Platform, f: impl FnOnce() -> R) -> R {
    let _restore = test_spelling::Override::set(p);
    f()
}

#[cfg(test)]
mod test_spelling {
    use super::Platform;
    use std::cell::Cell;

    thread_local! {
        static OVERRIDE: Cell<Option<Platform>> = const { Cell::new(None) };
    }

    pub fn current() -> Platform {
        OVERRIDE.with(Cell::get).unwrap_or(Platform::Mac)
    }

    /// Puts the previous override back when dropped (also when unwinding).
    pub struct Override(Option<Platform>);

    impl Override {
        pub fn set(p: Platform) -> Self {
            Self(OVERRIDE.with(|o| o.replace(Some(p))))
        }
    }

    impl Drop for Override {
        fn drop(&mut self) {
            OVERRIDE.with(|o| o.set(self.0));
        }
    }
}

/// App text in macOS terms (blyg-app's `platform_text::Text`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Text {
    mac: &'static str,
    windows: Option<&'static str>,
}

/// Text whose Windows wording is the macOS text respelled.
pub const fn ui_text(mac: &'static str) -> Text {
    Text { mac, windows: None }
}

/// Text Windows says differently (not just with other keys).
pub const fn ui_text_per_platform(mac: &'static str, windows: &'static str) -> Text {
    Text {
        mac,
        windows: Some(windows),
    }
}

impl Text {
    /// As the page shows it.
    pub fn get(self) -> &'static str {
        crate::keymap::hint(self.source(Platform::spelling()))
    }

    /// The text as written for `p`, before respelling.
    pub fn source(self, p: Platform) -> &'static str {
        match p {
            Platform::Mac | Platform::Browser => self.mac,
            Platform::Windows => self.windows.unwrap_or(self.mac),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_browser_spells_text_with_the_keys_it_binds() {
        assert_eq!(Platform::spelling(), Platform::Mac);
        assert_eq!(ui_text("⌘⏎ publishes").get(), "⌘⏎ publishes");
        assert_eq!(
            spelled_as(Platform::Browser, || ui_text("⌘⏎ publishes").get()),
            "⌃⏎ publishes"
        );
        assert_eq!(
            spelled_as(Platform::Windows, || ui_text("⌘⏎ publishes").get()),
            "Ctrl+Enter publishes"
        );
        assert_eq!(Platform::spelling(), Platform::Mac, "put back");
    }
}
