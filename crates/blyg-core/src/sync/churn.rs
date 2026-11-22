//! A list that changes under every walk of it (`CursorExpired` after
//! `CURSOR_RESTARTS` restarts) is left as held for that pull. A studio
//! expires a reading cursor on every post a subscription's backfill imports,
//! so a long list (the Background lens, a hopper's memberships) can expire
//! pull after pull until the import ends. Each such read costs the owner
//! every page it walked, so after [`STREAK`] in a row the list sits out
//! 1, 2, 4 … up to [`MAX_SKIP`] pulls (the safety net's doubling), and a
//! read that reaches its end puts it back to every pull.

/// Expiries in a row before a list starts sitting pulls out.
pub(crate) const STREAK: u32 = 3;

/// The most pulls a list sits out in one go.
pub(crate) const MAX_SKIP: u32 = 32;

/// One list's run of expiries.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub(crate) struct Churn {
    /// Expiries since the list was last read to its end.
    streak: u32,
    /// Pulls still to sit out.
    skip: u32,
    /// The last sit-out (0: none since the last read).
    wait: u32,
    /// Said once a session.
    reported: bool,
}

impl Churn {
    /// Whether this pull may read the list. A pull that may not counts down
    /// the sit-out, so ask only when the list would otherwise be read.
    pub(crate) fn due(&mut self) -> bool {
        if self.skip > 0 {
            self.skip -= 1;
            false
        } else {
            true
        }
    }

    /// The list was read to its end: every pull again.
    pub(crate) fn read(&mut self) {
        *self = Churn {
            reported: self.reported,
            ..Churn::default()
        };
    }

    /// The list changed under every walk again. True the first time a
    /// list starts sitting pulls out this session: say so, once.
    pub(crate) fn expired(&mut self) -> bool {
        self.streak += 1;
        if self.streak < STREAK {
            return false;
        }
        self.wait = (self.wait * 2).clamp(1, MAX_SKIP);
        self.skip = self.wait;
        !std::mem::replace(&mut self.reported, true)
    }
}

/// What to say, once a session, when `list` starts sitting pulls out.
pub(crate) fn message(list: &str) -> String {
    let what = match list {
        "background" => "the Background list",
        "hoppers" => "your hoppers",
        "signals" => "your thumbs",
        "shelf" => "the Shelf",
        _ => "a list",
    };
    format!(
        "Your blyg kept changing while Burrow read {what} (a subscription is \
         probably still importing posts). Burrow keeps what it has and reads it \
         again in a few minutes; until then new posts may show in the Feed \
         rather than Background."
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The pulls, out of `n`, on which the list is read when every read
    /// expires.
    fn reads_when_always_expiring(n: u32) -> Vec<u32> {
        let mut c = Churn::default();
        (0..n)
            .filter(|_| {
                let due = c.due();
                if due {
                    c.expired();
                }
                due
            })
            .collect()
    }

    #[test]
    fn a_few_expiries_change_nothing() {
        let mut c = Churn::default();
        for _ in 1..STREAK {
            assert!(!c.expired());
            assert!(c.due(), "read again next pull");
        }
    }

    #[test]
    fn a_list_that_always_expires_sits_out_doubling_pulls_up_to_the_cap() {
        let pulls = reads_when_always_expiring(400);
        // Read on each of the first STREAK pulls, then gaps of 1, 2, 4 …
        let gaps: Vec<u32> = pulls.windows(2).map(|w| w[1] - w[0] - 1).collect();
        assert_eq!(&gaps[..STREAK as usize - 1], &[0, 0]);
        assert_eq!(
            &gaps[STREAK as usize - 1..STREAK as usize + 5],
            &[1, 2, 4, 8, 16, 32]
        );
        assert!(gaps.iter().all(|&g| g <= MAX_SKIP), "bounded: {gaps:?}");
        assert_eq!(*gaps.last().unwrap(), MAX_SKIP);
        // The work is bounded too: a few reads an hour of minute-long pulls.
        assert!(pulls.len() < 25, "{} reads in 400 pulls", pulls.len());
    }

    #[test]
    fn a_read_to_the_end_puts_it_back_to_every_pull() {
        let mut c = Churn::default();
        for _ in 0..STREAK + 2 {
            while !c.due() {}
            c.expired();
        }
        c.read();
        assert!(c.due());
        for _ in 1..STREAK {
            c.expired();
            assert!(c.due(), "a fresh streak");
        }
        c.expired();
        assert!(!c.due(), "then one pull out, not where it left off");
        assert!(c.due());
    }

    #[test]
    fn it_is_said_once_a_session() {
        let mut c = Churn::default();
        let said: Vec<bool> = (0..STREAK + 3).map(|_| c.expired()).collect();
        assert_eq!(said.iter().filter(|&&s| s).count(), 1, "{said:?}");
        assert!(said[STREAK as usize - 1], "when it first sits out");
        c.read();
        let again = (0..STREAK + 3).any(|_| c.expired());
        assert!(!again, "not again after it settled");
    }
}
