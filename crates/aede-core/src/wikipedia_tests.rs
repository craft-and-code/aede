//! Tests for [`super`], split out of `wikipedia.rs`.
//!
//! The two fixtures below are abbreviated responses in the shape the two
//! endpoints answer with. They are here so that what the program *concludes*
//! from an answer is proven without a network — the same arrangement
//! `musicbrainz_tests.rs` uses.

use super::*;
use crate::json::parse;

/// `Special:EntityData/Q11649.json`, cut down to the parts that are read.
///
/// The real document is large — every statement, every label, every sitelink —
/// and none of the rest is looked at. A fixture that reproduced all of it
/// would prove nothing extra and hide what is actually being asserted.
const ENTITY: &str = r#"{
  "entities": {
    "Q11649": {
      "type": "item",
      "id": "Q11649",
      "labels": { "en": { "language": "en", "value": "Marilyn Manson" } },
      "sitelinks": {
        "commonswiki": { "site": "commonswiki", "title": "Category:Marilyn Manson" },
        "enwiki": {
          "site": "enwiki",
          "title": "Marilyn Manson (band)",
          "badges": []
        },
        "enwikiquote": { "site": "enwikiquote", "title": "Marilyn Manson" },
        "frwiki": { "site": "frwiki", "title": "Marilyn Manson (groupe)", "badges": [] }
      }
    }
  }
}"#;

/// `…/api/rest_v1/page/summary/Marilyn_Manson_(band)`, likewise abbreviated.
const SUMMARY: &str = r#"{
  "type": "standard",
  "title": "Marilyn Manson",
  "lang": "en",
  "extract": "Marilyn Manson is an American rock band formed in Fort Lauderdale, Florida, in 1989.",
  "content_urls": {
    "desktop": {
      "page": "https://en.wikipedia.org/wiki/Marilyn_Manson_(band)"
    },
    "mobile": {
      "page": "https://en.m.wikipedia.org/wiki/Marilyn_Manson_(band)"
    }
  }
}"#;

fn json(text: &str) -> crate::json::Json {
    parse(text).expect("the fixture is valid JSON")
}

#[test]
fn the_entity_id_is_read_from_the_end_of_whatever_shape_the_link_has() {
    for link in [
        "https://www.wikidata.org/wiki/Q11649",
        "https://www.wikidata.org/wiki/Q11649/",
        "http://www.wikidata.org/entity/Q11649",
    ] {
        assert_eq!(
            entity_id(link).as_deref(),
            Some("Q11649"),
            "MusicBrainz returns this relationship in more than one shape: {link}"
        );
    }
}

#[test]
fn a_link_that_is_not_an_entity_yields_nothing_rather_than_a_bad_request() {
    // Each of these would otherwise be sent out as a lookup that cannot
    // succeed, and a request that was never going to work reads, three steps
    // later, as the service refusing us.
    for link in [
        "https://www.wikidata.org/wiki/Property:P31",
        "https://www.wikidata.org/wiki/Special:EntityData",
        "https://en.wikipedia.org/wiki/Marilyn_Manson_(band)",
        "https://www.wikidata.org/wiki/Q",
        "https://www.wikidata.org/wiki/Q11a49",
        "",
    ] {
        assert_eq!(entity_id(link), None, "not an entity id: {link}");
    }
}

#[test]
fn the_first_language_that_has_an_article_wins() {
    let doc = json(ENTITY);
    assert_eq!(
        article(&doc, "Q11649", &["fr", "en"]),
        Some(Article {
            lang: "fr".to_string(),
            title: "Marilyn Manson (groupe)".to_string(),
        }),
        "the reader's own language is preferred when it has an article"
    );
    assert_eq!(
        article(&doc, "Q11649", &["de", "en"]),
        Some(Article {
            lang: "en".to_string(),
            title: "Marilyn Manson (band)".to_string(),
        }),
        "and the chain falls through to the next language, not to nothing"
    );
    assert_eq!(
        article(&doc, "Q11649", &["de", "es"]),
        None,
        "no article in any language asked for is an answer, not a failure"
    );
}

#[test]
fn a_sister_project_is_not_mistaken_for_an_encyclopaedia_article() {
    // `enwikiquote` sits in the same list and holds quotations, not a
    // description. Matching on a prefix rather than on the exact site name is
    // how a summary ends up being someone's collected sayings.
    let doc = json(ENTITY);
    let found = article(&doc, "Q11649", &["en"]).expect("an article");
    assert_eq!(found.title, "Marilyn Manson (band)");
}

#[test]
fn an_answer_about_another_id_is_still_read_when_it_is_unambiguous() {
    // Wikidata answers a redirected entity under the target's id. One entity
    // in the document means there is no question which one was meant.
    let doc = json(ENTITY);
    assert_eq!(
        article(&doc, "Q99999", &["en"]).map(|a| a.title),
        Some("Marilyn Manson (band)".to_string()),
        "a redirect is followed rather than reported as nothing"
    );
}

#[test]
fn a_title_becomes_an_address_without_leaving_anything_raw() {
    let url = summary_url(&Article {
        lang: "en".to_string(),
        title: "Marilyn Manson (band)".to_string(),
    });
    assert_eq!(
        url,
        "https://en.wikipedia.org/api/rest_v1/page/summary/Marilyn_Manson_%28band%29"
    );

    // Titles are not ASCII, and a raw byte in a path is where a request stops
    // being the one that was intended.
    let url = summary_url(&Article {
        lang: "fr".to_string(),
        title: "Édith Piaf".to_string(),
    });
    assert_eq!(
        url, "https://fr.wikipedia.org/api/rest_v1/page/summary/%C3%89dith_Piaf",
        "and the language chooses the host, not just the text"
    );
}

#[test]
fn the_paragraph_arrives_with_its_credit_or_not_at_all() {
    let asked = Article {
        lang: "en".to_string(),
        title: "Marilyn Manson (band)".to_string(),
    };
    let found = prose(&json(SUMMARY), &asked).expect("an extract");
    assert!(
        found
            .text
            .starts_with("Marilyn Manson is an American rock band")
    );
    assert_eq!(
        found.url, "https://en.wikipedia.org/wiki/Marilyn_Manson_(band)",
        "the canonical address is preferred: a title may have been redirected, \
         and a credit has to point where the reader will land"
    );
    assert_eq!(found.lang, "en");
    assert_eq!(found.licence, LICENCE);
}

#[test]
fn a_response_with_no_canonical_url_is_still_credited() {
    // The type cannot hold the words without the attribution, so a response
    // missing `content_urls` must produce an address rather than a `None` that
    // silently loses the paragraph.
    let asked = Article {
        lang: "fr".to_string(),
        title: "Édith Piaf".to_string(),
    };
    let found = prose(
        &json(r#"{"lang":"fr","extract":"Chanteuse française."}"#),
        &asked,
    )
    .expect("an extract");
    assert_eq!(found.url, "https://fr.wikipedia.org/wiki/%C3%89dith_Piaf");
    assert_eq!(found.licence, LICENCE);
}

#[test]
fn a_page_with_nothing_to_say_says_nothing() {
    let asked = Article {
        lang: "en".to_string(),
        title: "Manson".to_string(),
    };
    for body in [
        r#"{"type":"disambiguation","title":"Manson"}"#,
        r#"{"extract":"   "}"#,
        r#"{"extract":""}"#,
    ] {
        assert_eq!(
            prose(&json(body), &asked),
            None,
            "an empty extract is an absence, not a paragraph: {body}"
        );
    }
}

#[test]
fn the_language_of_the_answer_beats_the_language_that_was_asked() {
    // Asking a language edition can land on an article that declares another;
    // storing the one that was asked would label the text wrongly, and the
    // label is what tells a reader what they are about to read.
    let asked = Article {
        lang: "en".to_string(),
        title: "Whatever".to_string(),
    };
    let found = prose(&json(r#"{"lang":"fr","extract":"Un texte."}"#), &asked).expect("an extract");
    assert_eq!(found.lang, "fr");
}
