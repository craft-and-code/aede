use super::*;
#[test]
fn feed_blacklist_normalizes_safe_equivalent_spellings() {
    assert_eq!(
        canonical_feed("HTTPS://Feeds.Example.Test.:443/a/../%73how#latest").unwrap(),
        "https://feeds.example.test/show"
    );
    assert_eq!(
        canonical_feed("https://feeds.example.test?format=rss").unwrap(),
        "https://feeds.example.test/?format=rss"
    );
    assert_eq!(
        canonical_feed("https://[2001:0db8:0:0::1]/show").unwrap(),
        "https://[2001:db8::1]/show"
    );
    assert_ne!(
        canonical_feed("https://feeds.example.test/a%2fb").unwrap(),
        canonical_feed("https://feeds.example.test/a/b").unwrap()
    );
}
#[test]
fn malformed_urls_and_embedded_credentials_never_become_feed_identities() {
    for value in [
        "file:///private/feed.xml",
        "https://user:password@feeds.example.test/show",
        "https://feeds.example.test/%xx",
        "https://feeds.example.test\\show",
        "https://feeds.example.test:0/show",
        "https://feeds.example.test/\nshow",
    ] {
        assert!(canonical_feed(value).is_err(), "{value:?}");
    }
}
