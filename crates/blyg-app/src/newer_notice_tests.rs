//! The notice for a blyg on a newer studio than this Burrow knows
//! (`CoreEvent::ServerNewer`), in both platforms' words.

use super::{NEWER_TOAST, newer_toast_sub};
use crate::platform_text::{Platform, spelled_as};

#[test]
fn the_newer_studio_notice_says_what_burrow_does_in_plain_words() {
    assert_eq!(NEWER_TOAST, "Your blyg runs a newer studio");
    let mac = spelled_as(Platform::Mac, || newer_toast_sub("7.0.0"));
    assert_eq!(
        mac,
        "This Burrow doesn't know its API yet (contract 7.0.0). It keeps \
         syncing what it can, and won't remove anything on this Mac until \
         it's updated. Check for updates: Check for Updates… in the Burrow menu."
    );
}

#[test]
fn on_windows_it_names_this_pc_and_the_menu_button() {
    let win = spelled_as(Platform::Windows, || newer_toast_sub("next"));
    assert_eq!(
        win,
        "This Burrow doesn't know its API yet (contract next). It keeps \
         syncing what it can, and won't remove anything on this PC until \
         it's updated. Check for updates: Check for Updates… in the Menu button."
    );
}
