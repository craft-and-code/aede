use super::*;

#[test]
fn simple_round_trip() {
    let mut o = Json::obj();
    o.set("title", "Kind of Blue".into());
    o.set("year", 1959u32.into());
    o.set("live", false.into());
    o.set(
        "tracks",
        Json::Arr(vec!["So What".into(), "Freddie Freeloader".into()]),
    );
    let encoded = o.to_string_compact();
    let reparsed = parse(&encoded).expect("must reparse");
    assert_eq!(reparsed, o);
}

#[test]
fn escapes_and_accents() {
    let source = r#"{"a":"line\ncontinued","b":"café","c":"🎵"}"#;
    let v = parse(source).unwrap();
    assert_eq!(v.field_str("a").unwrap(), "line\ncontinued");
    assert_eq!(v.field_str("b").unwrap(), "café");
    assert_eq!(v.field_str("c").unwrap(), "🎵");
    // The round trip must be stable as well.
    let reparsed = parse(&v.to_string_compact()).unwrap();
    assert_eq!(reparsed, v);
}

#[test]
fn numbers_and_null() {
    let v = parse(r#"{"n":-12,"f":1.5,"e":2e3,"z":null}"#).unwrap();
    assert_eq!(v.get("n").unwrap().as_f64(), Some(-12.0));
    assert_eq!(v.get("f").unwrap().as_f64(), Some(1.5));
    assert_eq!(v.get("e").unwrap().as_f64(), Some(2000.0));
    assert_eq!(v.get("z"), Some(&Json::Null));
}

#[test]
fn rejects_trailing_data() {
    assert!(parse("{} {}").is_err());
    assert!(parse("{\"a\":}").is_err());
}

#[test]
fn pretty_output_is_reparsable() {
    let v = parse(r#"{"a":[1,2,{"b":"c"}],"d":{}}"#).unwrap();
    assert_eq!(parse(&v.to_string_pretty()).unwrap(), v);
}
