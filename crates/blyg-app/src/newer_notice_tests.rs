//! The notice for a blyg on a newer studio than this Burrow knows
//! (`CoreEvent::ServerNewer`), in both platforms' words.

use super::{NEWER_TOAST, newer_toast_sub};
use crate::platform_text::{Platform, spelled_as};

#[test]
fn the_newer_studio_notice_says_what_burrow_does_in_plain_words() {
    assert_eq!(NEWER_TOAST, "Your blyg runs a newer studio");
    let mac = spelled_as(Platform::Mac, newer_toast_sub);
    assert_eq!(
        mac,
        "Your blyg's studio is newer than this Burrow. Burrow keeps syncing what it \
         can, and removes nothing on this Mac. To get the newest Burrow: Check for \
         Updates… in the Burrow menu."
    );
    // User testing, 0.28: no contract numbers, and "update" once.
    assert!(!mac.contains("contract"), "{mac}");
    assert_eq!(mac.to_lowercase().matches("update").count(), 1, "{mac}");
}

/// Windows has no updater (`update::disabled_reason`): its last sentence
/// says where new builds come from instead.
#[test]
fn on_windows_it_names_this_pc_and_where_new_builds_are() {
    let win = spelled_as(Platform::Windows, newer_toast_sub);
    assert_eq!(
        win,
        "Your blyg's studio is newer than this Burrow. Burrow keeps syncing what it \
         can, and removes nothing on this PC. To get the newest Burrow, download it \
         from the Releases page on GitHub."
    );
    assert!(!win.contains("Check for Updates"), "{win}");
}
