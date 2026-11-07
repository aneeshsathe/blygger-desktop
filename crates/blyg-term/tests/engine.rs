//! The engine: bytes in, cells out.

use blyg_term::{Color, CursorShape, GridPoint, Rgb, new_engine};

#[test]
fn text_lands_in_cells() {
    let mut t = new_engine(20, 4);
    t.feed(b"hello\r\nworld");
    let s = t.snapshot();
    assert_eq!(s.cols, 20);
    assert_eq!(s.rows.len(), 4);
    assert_eq!(s.lines(), ["hello", "world", "", ""]);
    let c = s.cursor.expect("cursor shown");
    assert_eq!((c.row, c.col), (1, 5));
}

#[test]
fn colours_and_attributes() {
    let mut t = new_engine(20, 2);
    // Red bold, then 256-colour 196 underlined, then a truecolour background.
    t.feed(b"\x1b[1;31mR\x1b[0m\x1b[4;38;5;196mI\x1b[0m\x1b[48;2;1;2;3mB\x1b[0m.");
    let s = t.snapshot();
    let row = &s.rows[0].cells;
    assert_eq!(row[0].ch, 'R');
    assert_eq!(row[0].fg, Color::Indexed(1));
    assert!(row[0].attrs.bold && !row[0].attrs.underline);
    assert_eq!(row[1].fg, Color::Indexed(196));
    assert!(row[1].attrs.underline);
    assert_eq!(row[2].bg, Color::Rgb(Rgb::new(1, 2, 3)));
    assert_eq!(row[3].fg, Color::Default);
    assert_eq!(row[3].bg, Color::Default);
    // Inverse is reported, not applied: the app swaps the theme's colours.
    t.feed(b"\x1b[7mX");
    assert!(t.snapshot().rows[0].cells[4].attrs.inverse);
}

#[test]
fn the_cursor_moves_hides_and_changes_shape() {
    let mut t = new_engine(10, 5);
    t.feed(b"\x1b[3;4H");
    let c = t.snapshot().cursor.unwrap();
    assert_eq!((c.row, c.col, c.shape), (2, 3, CursorShape::Block));
    t.feed(b"\x1b[6 q"); // a steady bar
    assert_eq!(t.snapshot().cursor.unwrap().shape, CursorShape::Beam);
    t.feed(b"\x1b[?25l");
    assert_eq!(t.snapshot().cursor, None);
}

#[test]
fn resize_and_wrap() {
    let mut t = new_engine(10, 3);
    t.feed(b"0123456789AB");
    assert_eq!(t.snapshot().lines(), ["0123456789", "AB", ""]);
    t.resize(20, 5, 8, 16);
    let s = t.snapshot();
    assert_eq!(t.size(), (20, 5));
    assert_eq!(s.cols, 20);
    assert_eq!(s.rows.len(), 5);
    assert!(s.rows.iter().all(|r| r.cells.len() == 20));
    // The wrapped line reflows onto one row.
    assert!(
        s.lines().contains(&"0123456789AB".to_string()),
        "{:?}",
        s.lines()
    );
}

#[test]
fn scrollback_and_scrolling() {
    let mut t = new_engine(10, 3);
    for i in 0..10 {
        t.feed(format!("line{i}\r\n").as_bytes());
    }
    let s = t.snapshot();
    assert_eq!(s.lines(), ["line8", "line9", ""]);
    assert!(s.history >= 8);
    t.scroll(3);
    let s = t.snapshot();
    assert_eq!(s.display_offset, 3);
    assert_eq!(s.lines(), ["line5", "line6", "line7"]);
    // The cursor is below the view.
    assert_eq!(s.cursor, None);
    t.scroll_to_bottom();
    assert_eq!(t.snapshot().display_offset, 0);
}

#[test]
fn selection_text_spans_history_and_joins_wrapped_lines() {
    let mut t = new_engine(5, 4);
    t.feed(b"abcdefg\r\nxy  \r\nz");
    // "abcdefg" wraps after five columns: one logical line.
    let s = t.snapshot();
    assert_eq!(s.lines(), ["abcde", "fg", "xy", "z"]);
    let all = t.text_between(GridPoint { line: 0, col: 0 }, GridPoint { line: 3, col: 4 });
    assert_eq!(all, "abcdefg\nxy\nz");
    // Backwards selections read forwards.
    let back = t.text_between(GridPoint { line: 1, col: 1 }, GridPoint { line: 0, col: 3 });
    assert_eq!(back, "defg");
}

#[test]
fn queries_are_answered_through_replies() {
    let mut t = new_engine(10, 3);
    t.feed(b"ab\x1b[6n"); // where is the cursor?
    assert_eq!(t.take_replies(), b"\x1b[1;3R");
    assert!(t.take_replies().is_empty());
    t.set_default_colors(Rgb::new(0x11, 0x22, 0x33), Rgb::new(0xfa, 0xfb, 0xfc));
    t.feed(b"\x1b]11;?\x07"); // the background colour?
    let reply = String::from_utf8(t.take_replies()).unwrap();
    assert!(reply.contains("rgb:fafa/fbfb/fcfc"), "{reply:?}");
}

#[test]
fn modes_follow_the_program() {
    let mut t = new_engine(10, 3);
    assert!(!t.modes().app_cursor && !t.modes().bracketed_paste);
    t.feed(b"\x1b[?1h\x1b[?2004h\x1b[?1049h");
    let m = t.modes();
    assert!(m.app_cursor && m.bracketed_paste && m.alt_screen);
    assert_eq!(
        t.encode_key(&blyg_term::Key::named("up")).unwrap(),
        b"\x1bOA"
    );
}

#[test]
fn the_clipboard_is_out_of_reach() {
    let mut t = new_engine(10, 3);
    // OSC 52 read: no reply (nothing of the user's clipboard leaves).
    t.feed(b"\x1b]52;c;?\x07");
    assert!(t.take_replies().is_empty());
}

#[test]
fn wide_characters_take_two_cells() {
    let mut t = new_engine(10, 2);
    t.feed("a漢b".as_bytes());
    let row = &t.snapshot().rows[0];
    assert_eq!(row.cells[1].ch, '漢');
    assert!(row.cells[1].wide);
    assert!(row.cells[2].spacer);
    assert_eq!(row.cells[3].ch, 'b');
    assert_eq!(row.text(), "a漢b");
}
