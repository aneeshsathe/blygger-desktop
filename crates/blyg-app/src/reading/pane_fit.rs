//! --- ux 0.29 --- Which of the Reader's columns fit beside the open post.
//!
//! The Reader is sources | list | post, and the post can have a panel at
//! its right (Browsing history). In a narrow window those used to squeeze
//! the post to a sliver (55 px at 800 px with the history open). The post
//! keeps a minimum width: the list column makes way first, then the sources
//! pane (only while a side panel is open). Both come back as soon as there
//! is room again; nothing is closed.

/// The narrowest an open post gets while a column can make way for it.
pub const POST_MIN: f32 = 320.;

/// The narrowest the author's own site (a web page) gets.
pub const SITE_MIN: f32 = 480.;

/// The list column's share of the row (as `render_reading_screen` draws it).
pub fn list_share(sources_open: bool) -> f32 {
    if sources_open { 0.3 } else { 0.38 }
}

/// The columns drawn beside the post.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Fit {
    pub sources: bool,
    pub list: bool,
}

/// What fits in a row `room` wide: the sources pane `sources_w` wide (when
/// open), the list column, and a side panel `side_w` wide in the post's
/// pane, with the post at least `post_min` wide. With no post open the
/// list is what's read, so everything stays.
pub fn fit(room: f32, sources_w: Option<f32>, side_w: f32, post_min: f32, post_open: bool) -> Fit {
    let mut f = Fit {
        sources: sources_w.is_some(),
        list: true,
    };
    if !post_open {
        return f;
    }
    let src = sources_w.unwrap_or(0.);
    let list = room * list_share(sources_w.is_some());
    if room - src - list - side_w >= post_min {
        return f;
    }
    f.list = false;
    if room - src - side_w < post_min && side_w > 0. {
        f.sources = false;
    }
    f
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn everything_fits_in_a_wide_window() {
        let all = Fit {
            sources: true,
            list: true,
        };
        assert_eq!(fit(1066., Some(210.), 0., POST_MIN, true), all);
        // 800 px, no panel: 766 − 210 − 230 = 326 for the post.
        assert_eq!(fit(766., Some(210.), 0., POST_MIN, true), all);
        // The author's site wants more: the list column makes way.
        assert_eq!(
            fit(766., Some(210.), 0., SITE_MIN, true),
            Fit {
                sources: true,
                list: false
            }
        );
    }

    #[test]
    fn a_side_panel_takes_the_list_then_the_sources() {
        // 1100 px with the history (270): the list makes way.
        assert_eq!(
            fit(1066., Some(210.), 270., POST_MIN, true),
            Fit {
                sources: true,
                list: false
            }
        );
        // 800 px: the sources too (766 − 270 = 496 for the post).
        assert_eq!(
            fit(766., Some(210.), 270., POST_MIN, true),
            Fit {
                sources: false,
                list: false
            }
        );
    }

    #[test]
    fn nothing_moves_with_no_post_open() {
        let f = fit(500., Some(210.), 270., POST_MIN, false);
        assert!(f.sources && f.list);
    }
}
