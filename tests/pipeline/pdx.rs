use super::*;

#[test]
fn parser_preserves_source_and_accepts_engine_syntax() {
    let source = "\u{feff}# { ignored\r\nguiTypes = { position { x = -12 y = @[ a * 2 - 4 ] } size = { width = 100% height = @h } text = \"кириллица } # \\\"\" unknown = { 1 2 3 } }\r\n";
    let doc = Document::parse(source).unwrap();
    assert_eq!(Edits::default().apply(doc.source).unwrap(), source);
    assert_eq!(
        doc.nodes[0].children()[0].children()[1].atom(),
        Some("@[ a * 2 - 4 ]")
    );
    for bad in ["x = {", "x = \"oops", "x = @[ a", "}", "x ="] {
        assert!(Document::parse(bad).is_err(), "{bad}");
    }
}

#[test]
fn parser_preserves_escaped_quotes_and_backslash_pairs() {
    for value in [
        r"gfx\\",
        r"gfx\file",
        r#"say \"hi\""#,
        r#"gfx\\\""#,
        "Текст\\\\",
    ] {
        let source = format!("value = \"{value}\" next = yes");
        let doc = Document::parse(&source).unwrap();
        assert_eq!(doc.nodes.len(), 2, "{source}");
        assert_eq!(doc.nodes[0].atom(), Some(value));
        assert_eq!(doc.nodes[1].key, Some("next"));
        assert_eq!(doc.nodes[1].atom(), Some("yes"));
        assert_eq!(Edits::default().apply(&source).unwrap(), source);
    }
    assert!(Document::parse(r#"value = "unfinished\""#).is_err());
}

#[test]
fn parser_distinguishes_operators_from_quoted_atoms_and_anonymous_blocks() {
    for operator in ["=", "<", ">", "<=", ">=", "!="] {
        let source = format!("trigger {{ value {operator} 2 quoted = \"{operator}\" }}");
        let doc = Document::parse(&source).unwrap();
        let children = doc.nodes[0].children();
        assert_eq!(children[0].key, Some("value"));
        assert_eq!(children[0].atom(), Some("2"));
        assert_eq!(children[1].atom(), Some(operator));
        assert_eq!(Edits::default().apply(&source).unwrap(), source);
    }
    let doc = Document::parse("values = { { { x = 1 } } { x = 2 } }").unwrap();
    let children = doc.nodes[0].children();
    assert!(children.iter().all(|node| node.key.is_none()));
    assert_eq!(
        children[0].children()[0].field("x").unwrap().atom(),
        Some("1")
    );
    assert_eq!(children[1].field("x").unwrap().atom(), Some("2"));
    for invalid in ["= 1", "< 1", "x = >", "x <", "x = y = z", "x = { >= 1 }"] {
        assert!(Document::parse(invalid).is_err(), "{invalid}");
    }
}

#[test]
fn document_walk_preserves_order_and_stops_at_the_first_error() {
    let doc =
        Document::parse("first = { child = 1 fail = { skipped = 2 } after = 3 } last = 4").unwrap();
    let mut visited = Vec::new();
    doc.walk(|node| visited.push(node.key.unwrap()));
    assert_eq!(
        visited,
        ["first", "child", "fail", "skipped", "after", "last"]
    );
    visited.clear();
    let result = doc.try_walk(|node| {
        let key = node.key.unwrap();
        visited.push(key);
        if key == "fail" { Err("stop") } else { Ok(()) }
    });
    assert_eq!(result, Err("stop"));
    assert_eq!(visited, ["first", "child", "fail"]);
}

#[test]
fn edits_reject_overlap_and_preserve_unicode() {
    let mut edits = Edits::default();
    edits.replace(0..2, "a");
    edits.replace(1..3, "b");
    assert!(edits.apply("abcd").is_err());
    let mut edits = Edits::default();
    edits.replace(1..2, "a");
    assert!(edits.apply("Я").is_err());
    for (start, end) in [(0, 1), (2, 1), (0, 3), (3, 3)] {
        let mut edits = Edits::default();
        edits.replace(start..end, "a");
        assert!(edits.apply("Я").is_err());
    }
    let mut edits = Edits::default();
    edits.replace(0..2, "a");
    edits.replace(2..2, "б");
    assert_eq!(edits.apply("Я").unwrap(), "aб");
}
