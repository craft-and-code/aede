use super::*;

#[test]
fn xml_escapes_attributes_text_and_controls_without_creating_elements() {
    let payload = json!({"artists": {"index": [{"name": "<&\"'\n", "artist": [{"id": "safe", "name": "A\u{0}\u{1}B"}]}]},
        "genres": {"genre": [{"value": "Rock & <Jazz>", "songCount": 2}]}});
    let mut xml = String::new();
    xml_element(&mut xml, "subsonic-response", &payload, true);
    assert!(xml.contains("xmlns=\"http://subsonic.org/restapi\""));
    assert!(xml.contains("name=\"&lt;&amp;&quot;&apos;&#10;\""));
    assert!(xml.contains("name=\"A��B\""));
    assert!(xml.contains("<genre songCount=\"2\">Rock &amp; &lt;Jazz&gt;</genre>"));
    assert!(!xml.contains("\u{0}"));
}

#[test]
fn sha256_and_utc_dates_have_stable_absolute_values() {
    assert_eq!(
        sha256("abc").unwrap(),
        "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
    );
    assert_eq!(utc_date(0).unwrap(), "1970-01-01T00:00:00Z");
    assert_eq!(utc_date(951_782_400).unwrap(), "2000-02-29T00:00:00Z");
    assert_eq!(utc_date(1_700_000_000).unwrap(), "2023-11-14T22:13:20Z");
    assert_eq!(utc_date(253_402_300_799).unwrap(), "9999-12-31T23:59:59Z");
    assert!(utc_date(u64::MAX).is_err());
}
