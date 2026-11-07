//! --- chat --- A system notification for a mention.
//!
//! macOS: a User Notifications banner (UNUserNotificationCenter; the first
//! one asks permission), only from inside Burrow.app, since the framework
//! refuses a process with no bundle. In every case the Dock icon bounces
//! once while Burrow isn't the active app. Windows: nothing yet; the tab's
//! marks still show.

/// Post `title` / `body`. `inactive`: Burrow isn't the frontmost app.
pub fn post(title: &str, body: &str, inactive: bool) {
    #[cfg(target_os = "macos")]
    mac::post(title, body, inactive);
    #[cfg(not(target_os = "macos"))]
    let _ = (title, body, inactive);
}

#[cfg(target_os = "macos")]
mod mac {
    use std::sync::Once;
    use std::sync::atomic::{AtomicU64, Ordering};

    use block2::RcBlock;
    use objc2::msg_send;
    use objc2::runtime::{AnyClass, AnyObject, Bool};
    use objc2_foundation::NSString;

    #[link(name = "UserNotifications", kind = "framework")]
    unsafe extern "C" {}

    /// NSInformationalRequest: one bounce.
    const INFORMATIONAL: usize = 10;
    /// UNAuthorizationOptionSound | UNAuthorizationOptionAlert.
    const SOUND_AND_ALERT: usize = 2 | 4;

    pub fn post(title: &str, body: &str, inactive: bool) {
        if inactive {
            bounce();
        }
        if !std::env::current_exe().is_ok_and(|p| crate::platform::is_bundled(&p)) {
            return;
        }
        banner(title, body);
    }

    fn bounce() {
        // SAFETY: AppKit on the main thread (the window's update), nil-checked.
        unsafe {
            let Some(cls) = AnyClass::get(c"NSApplication") else {
                return;
            };
            let app: *mut AnyObject = msg_send![cls, sharedApplication];
            if !app.is_null() {
                let _: isize = msg_send![app, requestUserAttention: INFORMATIONAL];
            }
        }
    }

    fn banner(title: &str, body: &str) {
        static ASKED: Once = Once::new();
        static SEQ: AtomicU64 = AtomicU64::new(0);
        // SAFETY: UserNotifications calls with nil checks; the strings and
        // the content object outlive the calls that copy them.
        unsafe {
            let (Some(center_cls), Some(content_cls), Some(request_cls)) = (
                AnyClass::get(c"UNUserNotificationCenter"),
                AnyClass::get(c"UNMutableNotificationContent"),
                AnyClass::get(c"UNNotificationRequest"),
            ) else {
                return;
            };
            let center: *mut AnyObject = msg_send![center_cls, currentNotificationCenter];
            if center.is_null() {
                return;
            }
            ASKED.call_once(|| {
                let done = RcBlock::new(|_granted: Bool, _error: *mut AnyObject| {});
                let _: () = msg_send![
                    center,
                    requestAuthorizationWithOptions: SOUND_AND_ALERT,
                    completionHandler: &*done
                ];
            });
            let content: *mut AnyObject = msg_send![content_cls, new];
            if content.is_null() {
                return;
            }
            let t = NSString::from_str(title);
            let b = NSString::from_str(body);
            let _: () = msg_send![content, setTitle: &*t];
            let _: () = msg_send![content, setBody: &*b];
            let id = NSString::from_str(&format!(
                "org.blygger.desktop.chat.{}",
                SEQ.fetch_add(1, Ordering::Relaxed)
            ));
            let nil: *mut AnyObject = std::ptr::null_mut();
            let request: *mut AnyObject = msg_send![
                request_cls,
                requestWithIdentifier: &*id,
                content: content,
                trigger: nil
            ];
            if !request.is_null() {
                let _: () = msg_send![
                    center,
                    addNotificationRequest: request,
                    withCompletionHandler: nil
                ];
            }
            let _: () = msg_send![content, release];
        }
    }
}
