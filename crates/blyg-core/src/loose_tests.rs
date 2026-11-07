use super::*;

#[test]
fn operators_come_off_the_query_and_words_stay() {
    let q = Query::parse("tide  is:orphan pools kind:thread");
    assert_eq!(q.ops, [Op::Orphan, Op::Kind(Kind::Thread)]);
    assert_eq!(q.text, "tide pools");
    assert!(!q.is_operator_only());

    let q = Query::parse("is:draft has:tk");
    assert!(q.is_operator_only());
    assert!(is_operator_only(" IS:Stale "));
    assert_eq!(search_words("v:3 gardens"), "gardens");
}

#[test]
fn without_operators_the_query_is_untouched() {
    for q in ["", "  tide  pools ", "is:blue", "a:b c", "links:", "v:x"] {
        let p = Query::parse(q);
        assert!(p.ops.is_empty(), "{q}");
        assert_eq!(p.text, q);
        assert!(!p.is_operator_only());
    }
}

#[test]
fn targets_are_ids_or_hosts() {
    assert_eq!(
        Query::parse("links:01J9QK3").ops,
        [Op::Links(Target::Id("01j9qk3".into()))]
    );
    assert_eq!(
        Query::parse("quotes:https://www.Lin.example.org/t/x").ops,
        [Op::Quotes(Target::Host("lin.example.org".into()))]
    );
    assert_eq!(Query::parse("v:12").ops, [Op::Version(12)]);
    let h = Target::Host("example.org".into());
    assert!(h.host_matches("https://lin.example.org/x"));
    assert!(!h.host_matches("https://notexample.org"));
}

#[test]
fn web_links_are_found_in_text() {
    let md = "see https://a.example.com/x) and [b](http://b.example.net/y) not httpish";
    let v: Vec<_> = web_links(md).collect();
    assert_eq!(v, ["https://a.example.com/x", "http://b.example.net/y"]);
}
