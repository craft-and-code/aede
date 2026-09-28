use super::*;

fn person(mbid: &str, name: &str) -> SourcedContributor {
    SourcedContributor {
        mbid: mbid.into(),
        name: name.into(),
        names: vec![name.into()],
        recording_ids: vec![0],
        release_ids: vec![],
        work_mbids: vec![],
    }
}

#[test]
fn a_source_exact_name_beats_an_unrelated_local_partial_name() {
    let catalog = Catalog {
        artists: vec![Artist {
            id: 0,
            name: "Teo Macero Ensemble".into(),
            key: text::normalize("Teo Macero Ensemble"),
            ..Default::default()
        }],
        ..Default::default()
    };
    let sourced = [person("teo-id", "Teo Macero")];
    assert!(matches!(
        resolve_artist(&catalog, &sourced, "Teo Macero").unwrap(),
        ArtistPage::Sourced(found) if found.mbid == "teo-id"
    ));
}

#[test]
fn two_same_named_source_identities_require_an_identifier() {
    let catalog = Catalog::default();
    let sourced = [
        person("first-id", "Alex Smith"),
        person("second-id", "Alex Smith"),
    ];
    let error = match resolve_artist(&catalog, &sourced, "Alex Smith") {
        Ok(_) => panic!("same names must be ambiguous"),
        Err(error) => error.to_string(),
    };
    assert!(error.contains("first-id"));
    assert!(error.contains("second-id"));
    assert!(matches!(
        resolve_artist(&catalog, &sourced, "second-id").unwrap(),
        ArtistPage::Sourced(found) if found.mbid == "second-id"
    ));
}

#[test]
fn an_existing_local_name_keeps_its_page_when_a_source_names_a_possible_match() {
    let catalog = Catalog {
        artists: vec![Artist {
            id: 0,
            name: "Andrew Watt".into(),
            key: text::normalize("Andrew Watt"),
            ..Default::default()
        }],
        ..Default::default()
    };
    let sourced = [person("source-watt", "Andrew Watt")];
    assert!(matches!(
        resolve_artist(&catalog, &sourced, "Andrew Watt").unwrap(),
        ArtistPage::Local(found) if found.id == 0
    ));
    assert!(matches!(
        resolve_artist(&catalog, &sourced, "source-watt").unwrap(),
        ArtistPage::Sourced(found) if found.mbid == "source-watt"
    ));
}
