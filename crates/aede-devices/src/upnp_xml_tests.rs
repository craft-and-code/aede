use super::*;

#[test]
fn device_xml_resolves_namespaces_and_decodes_only_standard_entities() {
    let document = parse("<?xml version=\"1.0\"?><root xmlns=\"urn:root\" xmlns:s=\"urn:service\"><s:name>A &amp; B &#x1F3B5;<![CDATA[ <live>]]></s:name><empty/></root>").unwrap();
    assert_eq!(document.namespace, "urn:root");
    let name = document.child("name").unwrap();
    assert_eq!(name.namespace, "urn:service");
    assert_eq!(name.text, "A & B 🎵 <live>");
    assert_eq!(document.child("empty").unwrap().namespace, "urn:root");
    assert_eq!(escape("<&\"'>").unwrap(), "&lt;&amp;&quot;&apos;&gt;");
}

#[test]
fn malformed_and_entity_expanding_xml_is_refused_without_panicking() {
    for value in [
        "<!DOCTYPE root [<!ENTITY x 'secret'>]><root>&x;</root>",
        "<root>&custom;</root>",
        "<root>&#0;</root>",
        "<root>&#xD800;</root>",
        "<root><nested></root>",
        "<root/><root/>",
        "<root attr='1' attr='2'/>",
        "<root><unknown:value/></root>",
        "<!--><root/>",
        "<root>",
        "<root><!--</root>",
        "<root attr='<'/>",
        "<root><?process x?></root>",
        "<root>]]></root>",
    ] {
        assert!(parse(value).is_err(), "accepted {value}");
    }
    assert!(escape("control\u{1b}").is_err());
}

#[test]
fn nested_or_duplicate_action_values_are_not_flattened() {
    assert!(
        parse("<root><Value>1</Value><Value>2</Value></root>")
            .unwrap()
            .value("Value")
            .is_err()
    );
    assert!(
        parse("<root><Value><nested>1</nested></Value></root>")
            .unwrap()
            .value("Value")
            .is_err()
    );
}

#[test]
fn device_xml_bounds_size_nesting_and_nodes() {
    assert!(
        parse(&format!(
            "<root xmlns='{}'><child/></root>",
            "n".repeat(257)
        ))
        .is_err()
    );
    assert!(parse(&format!("<root>{}</root>", "a".repeat(MAX_BYTES))).is_err());
    assert!(
        parse(&format!(
            "{}{}",
            "<a>".repeat(MAX_DEPTH + 1),
            "</a>".repeat(MAX_DEPTH + 1)
        ))
        .is_err()
    );
    assert!(parse(&format!("<root>{}</root>", "<a/>".repeat(MAX_NODES))).is_err());
}
