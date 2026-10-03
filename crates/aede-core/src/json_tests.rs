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

#[test]
fn malformed_unicode_and_controls_are_errors_without_panicking() {
    for input in [
        r#""\uD800\u0000""#,
        r#""\uD800""#,
        r#""\uDC00""#,
        r#""\uD800\uD800""#,
        r#""\uD800x""#,
        r#""\u12G4""#,
        "\"line\nfeed\"",
        "\"tab\there\"",
        "\"\u{0000}\"",
    ] {
        let result = std::panic::catch_unwind(|| parse(input));
        assert!(result.is_ok(), "parser panicked for {input:?}");
        assert!(result.unwrap().is_err(), "accepted {input:?}");
    }
    assert_eq!(parse(r#""\uD83C\uDFB5""#).unwrap().as_str(), Some("🎵"));
}

#[test]
fn malformed_or_overflowing_numbers_are_rejected() {
    for input in ["01", "-01", "1.", "-.1", ".1", "1e", "1e+", "1e999"] {
        assert!(parse(input).is_err(), "accepted {input}");
    }
    for input in ["0", "-0", "1.0", "0.125", "-2.5e-3", "1E+4"] {
        assert!(parse(input).is_ok(), "refused {input}");
    }
}

#[test]
fn integer_fields_refuse_fractional_and_out_of_range_values() {
    for number in [
        -1.0,
        1.9,
        f64::INFINITY,
        f64::NAN,
        18_446_744_073_709_551_616.0,
    ] {
        assert_eq!(Json::Num(number).as_u64(), None, "accepted {number}");
    }
    assert_eq!(Json::Num(4_294_967_295.0).as_u32(), Some(u32::MAX));
    assert_eq!(Json::Num(4_294_967_296.0).as_u32(), None);
    assert_eq!(Json::Num(4_294_967_298.0).as_u32(), None);
    assert_eq!(Json::Num(1.0).as_u64(), Some(1));
}

#[test]
fn excessive_nesting_is_a_parse_error() {
    let input = format!("{}0{}", "[".repeat(256), "]".repeat(256));
    assert!(parse(&input).is_err());
    let input = format!("{}0{}", "[".repeat(32), "]".repeat(32));
    assert!(parse(&input).is_ok());
}
