use crate::json::Json;
use std::collections::BTreeMap;

#[test]
fn bounded_pretty_output_preserves_existing_keys_escaping_numbers_and_indentation() {
    let value = Json::Obj(BTreeMap::from([
        (
            "clé\n\"\\".into(),
            Json::Arr(vec![
                Json::Str("é\t\r\n\u{08}\u{0c}\u{01}\\\"".into()),
                Json::Null,
                Json::Bool(true),
            ]),
        ),
        (
            "counts".into(),
            Json::Arr(vec![
                Json::Num(42.0),
                Json::Num(-0.5),
                Json::Num(1.0e30),
                Json::Num(f64::INFINITY),
            ]),
        ),
        ("empty".into(), Json::Obj(BTreeMap::new())),
    ]));
    let expected = value.to_string_pretty();
    assert_eq!(
        value.to_string_pretty_bounded(expected.len()).unwrap(),
        expected
    );
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&expected).unwrap()["counts"],
        serde_json::json!([42, -0.5, 1.0e30, null])
    );
    assert!(value.to_string_pretty_bounded(expected.len() - 1).is_err());
    assert!(value.check_pretty_size(expected.len()).is_ok());
    assert!(value.check_pretty_size(expected.len() - 1).is_err());
}

#[test]
fn output_budget_counts_utf8_and_escaped_bytes_and_never_returns_partial_json() {
    for value in [
        Json::Str("💿".into()),
        Json::Str("\t".repeat(1024)),
        Json::Arr(vec![Json::Str("é".into()); 100]),
    ] {
        let full = value.to_string_pretty();
        for budget in [0, 1, full.len() / 2, full.len() - 1] {
            assert!(value.to_string_pretty_bounded(budget).is_err());
            assert!(value.check_pretty_size(budget).is_err());
        }
        assert_eq!(value.to_string_pretty_bounded(full.len()).unwrap(), full);
        assert!(value.check_pretty_size(full.len()).is_ok());
    }
}
