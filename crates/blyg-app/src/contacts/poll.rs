//! --- contacts --- A subscription's poll status on the Subscriptions
//! screen, as Studio's subscription row and Inspector show it: when your
//! blyg last polled it, how many polls in a row failed, and its flags. The
//! screen reads the subscriptions again each time it opens.

use blyg_core::{Subscription, SubscriptionKind};
use gpui_kit::*;

use crate::app::MainView;

/// The row's status line, and whether it's a problem (alert colour).
/// Studio's `sourceStatus`, with relative times as Burrow shows them.
pub(crate) fn status_line(s: &Subscription, now: chrono::DateTime<chrono::Utc>) -> (String, bool) {
    let polled = match s.poll.last_poll_at.as_deref() {
        Some(at) => match crate::vm::relative_time(at, now) {
            t if t.is_empty() => format!("last polled {at}"),
            t => format!("last polled {t}"),
        },
        None => "never polled".to_string(),
    };
    if s.status == "paused" {
        return ("paused".into(), false);
    }
    let flags = flags(s);
    let line = match s.poll.fail_count {
        0 => {
            let rss = if s.kind == SubscriptionKind::Rss {
                "legacy RSS · "
            } else {
                ""
            };
            format!("{rss}{polled}")
        }
        1 => format!("the last poll failed · {polled}"),
        n => format!("{n} polls failed in a row · {polled}"),
    };
    let line = match flags {
        Some(f) => format!("{line} · {f}"),
        None => line,
    };
    (line, s.poll.fail_count > 0)
}

/// The flags, `type: detail` each (Studio's Inspector).
fn flags(s: &Subscription) -> Option<String> {
    let f: Vec<String> = s
        .poll
        .flags
        .iter()
        .map(|f| match f.detail.as_deref().filter(|d| !d.is_empty()) {
            Some(d) => format!("{}: {d}", f.kind.replace('_', " ")),
            None => f.kind.replace('_', " "),
        })
        .collect();
    (!f.is_empty()).then(|| f.join(" · "))
}

impl MainView {
    /// Hook (the Subscriptions screen opening): read the subscriptions
    /// again, for their latest poll status. The list shows what's held
    /// meanwhile; a failed read keeps it.
    pub(crate) fn refresh_poll_status(&mut self, cx: &mut Context<Self>) {
        let backend = self.backend.clone();
        let task = cx.background_spawn(async move { backend.refresh_subscriptions() });
        cx.spawn(async move |this, cx| {
            if task.await.is_err() {
                return;
            }
            let _ = this.update(cx, |v, cx| {
                if v.reading.view == super::super::View::Subscriptions {
                    v.reading.subs = v.backend.subscriptions();
                    v.reading.sub_sel = v
                        .reading
                        .sub_sel
                        .min(v.reading.subs.len().saturating_sub(1));
                    cx.notify();
                }
            });
        })
        .detach();
    }
}

#[cfg(test)]
mod tests {
    use super::status_line;
    use blyg_core::{Subscription, SubscriptionKind};
    use blyg_core::{SubscriptionFlag, SubscriptionPoll};

    fn sub(fails: u32, at: Option<&str>) -> Subscription {
        Subscription {
            id: "s".into(),
            kind: SubscriptionKind::Blyg,
            origin: "https://lin.example.org/".into(),
            feed_url: "https://lin.example.org/feed.json".into(),
            title: "Lin".into(),
            status: "active".into(),
            in_blogroll: false,
            title_follows_source: true,
            poll: SubscriptionPoll {
                last_poll_at: at.map(str::to_string),
                fail_count: fails,
                flags: vec![],
            },
        }
    }

    #[test]
    fn the_line_says_when_and_whether_it_failed() {
        let now = chrono::Utc::now();
        let at = (now - chrono::Duration::minutes(5)).to_rfc3339();
        let (line, bad) = status_line(&sub(0, Some(&at)), now);
        assert!(line.starts_with("last polled "), "{line}");
        assert!(!bad);
        assert_eq!(status_line(&sub(0, None), now).0, "never polled");
        let (line, bad) = status_line(&sub(3, None), now);
        assert_eq!(line, "3 polls failed in a row · never polled");
        assert!(bad);
        let mut paused = sub(2, None);
        paused.status = "paused".into();
        assert_eq!(status_line(&paused, now), ("paused".into(), false));
        let mut flagged = sub(0, None);
        flagged.kind = SubscriptionKind::Rss;
        flagged.poll.flags = vec![SubscriptionFlag {
            kind: "site_mismatch".into(),
            at: at.clone(),
            detail: Some("feed says notes.example.org".into()),
        }];
        assert_eq!(
            status_line(&flagged, now).0,
            "legacy RSS · never polled · site mismatch: feed says notes.example.org"
        );
    }
}
