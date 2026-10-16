use super::*;
use aede_core::{model, store, tags};

struct Directory(std::path::PathBuf);

impl Drop for Directory {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn populated_catalog() -> model::Catalog {
    let files = (0..17)
        .map(|index| {
            let mut tags = tags::RawTags::default();
            tags.insert("title", format!("Song {index}"));
            tags.insert("artist", "Artist");
            tags.insert("album", "Album");
            model::ScannedFile {
                path: format!("/fictional/Album/{index}.flac"),
                size: 1,
                mtime: 1,
                tags,
                folder_cover: None,
                sidecar: None,
                integrity: None,
                fingerprint: None,
            }
        })
        .collect();
    model::build(files, vec!["/fictional".into()], 1, &[])
}

#[test]
fn empty_relations_keep_json_output_and_validate_windows_without_changing_stores() {
    let directory = Directory(
        std::env::temp_dir().join(format!("aede-empty-relations-{}", std::process::id())),
    );
    std::fs::create_dir_all(&directory.0).unwrap();
    let populated = populated_catalog();
    assert!(!graph::edges(&populated, &Default::default()).is_empty());
    let output = directory.0.join("relations.json");
    let arguments = |words: &[&str]| {
        let mut arguments = vec![
            "relations".to_string(),
            format!("--data={}", directory.0.display()),
            format!("--output={}", output.display()),
        ];
        arguments.extend(words.iter().map(|word| word.to_string()));
        Args::parse(arguments)
    };
    let snapshots = || {
        [
            "catalog.json",
            "conclusions.json",
            "sources.json",
            "user.json",
        ]
        .map(|name| std::fs::read(directory.0.join(name)).ok())
    };

    for (catalog, has_edges) in [(&populated, true), (&model::Catalog::default(), false)] {
        store::save(catalog, &store::catalog_path(&directory.0)).unwrap();
        let before = snapshots();
        for words in [
            vec!["NoSuchEndpoint", "--json"],
            vec!["--json", "--offset=999999"],
        ] {
            std::fs::write(&output, "preserve the prior report").unwrap();
            relations(&arguments(&words)).unwrap();
            assert_eq!(
                aede_core::json::parse(&std::fs::read_to_string(&output).unwrap()).ok(),
                Some(Json::Arr(Vec::new())),
                "an empty selection must replace the report with JSON: {words:?}"
            );
        }
        for invalid in [
            vec!["--limit=0"],
            vec!["--offset=invalid"],
            vec!["--all", "--limit=1"],
        ] {
            for format in [None, Some("--json")] {
                std::fs::write(&output, "preserve the prior report").unwrap();
                let mut words = vec!["NoSuchEndpoint"];
                words.extend(invalid.iter().copied());
                words.extend(format);
                assert!(relations(&arguments(&words)).is_err(), "accepted {words:?}");
                assert_eq!(
                    std::fs::read_to_string(&output).unwrap(),
                    "preserve the prior report"
                );
            }
        }
        if has_edges {
            relations(&arguments(&["--json", "--limit=1"])).unwrap();
            let rows = aede_core::json::parse(&std::fs::read_to_string(&output).unwrap()).unwrap();
            assert!(matches!(rows, Json::Arr(rows) if rows.len() == 1));
        }
        assert_eq!(snapshots(), before);
    }
}

#[test]
fn ambiguous_live_relations_cannot_remove_an_annotation_as_if_it_were_orphaned() {
    let directory = Directory(
        std::env::temp_dir().join(format!("aede-ambiguous-relations-{}", std::process::id())),
    );
    std::fs::create_dir_all(&directory.0).unwrap();
    let catalog = populated_catalog();
    store::save(&catalog, &store::catalog_path(&directory.0)).unwrap();
    let edges = graph::edges(&catalog, &Default::default());
    // More than sixteen distinct hex IDs guarantees a shared first digit,
    // without relying on a platform-specific hash prefix for one fixture.
    assert!(edges.len() > 16);
    let reference = edges
        .iter()
        .find(|edge| {
            let prefix = edge.reference.id()[..1].to_string();
            edges
                .iter()
                .filter(|other| other.reference.id().starts_with(&prefix))
                .count()
                > 1
        })
        .expect("the fixture has distinct live edges sharing a prefix")
        .reference
        .clone();
    let id = reference.id();
    let prefix = &id[..1];
    assert!(
        graph::select(&edges, prefix)
            .unwrap_err()
            .contains("ambiguous")
    );
    let mut orphan = reference.clone();
    orphan.kind = "missing_relationship".into();
    assert!(graph::select(&edges, &orphan.id()).is_err());
    let path = user::user_path(&directory.0);
    let mut data = user::UserData::default();
    for owner in [LOCAL_USER, "other"] {
        data.relation_entry(owner, &reference, 1).note = Some("keep this context".into());
    }
    user::save(&data, &path).unwrap();
    let before = std::fs::read(&path).unwrap();
    let catalog_before = std::fs::read(store::catalog_path(&directory.0)).unwrap();
    let arguments = |selector: &str| {
        Args::parse([
            "relation".to_string(),
            format!("--data={}", directory.0.display()),
            selector.to_string(),
            "--remove".to_string(),
        ])
    };
    for selector in [
        prefix.to_string(),
        format!(" {} ", prefix.to_ascii_uppercase()),
    ] {
        let error = relation(&arguments(&selector))
            .expect_err("a live ambiguous selector cannot choose a saved annotation")
            .to_string();
        assert!(error.contains("ambiguous"), "{error}");
        assert_eq!(std::fs::read(&path).unwrap(), before);
    }

    // Keep the genuine orphan control separate from the ambiguous selector:
    // its own hash must not influence which saved annotation that selects.
    for owner in [LOCAL_USER, "other"] {
        data.relation_entry(owner, &orphan, 1).note = Some("keep this context".into());
    }
    user::save(&data, &path).unwrap();
    relation(&arguments(&format!(" {} ", id.to_ascii_uppercase()))).unwrap();
    let remaining = user::load(&path).unwrap().unwrap();
    assert!(remaining.find_relation(LOCAL_USER, &reference).is_none());
    assert!(remaining.find_relation("other", &reference).is_some());
    assert!(remaining.find_relation(LOCAL_USER, &orphan).is_some());

    relation(&arguments(&orphan.id())).unwrap();
    let remaining = user::load(&path).unwrap().unwrap();
    assert!(remaining.find_relation(LOCAL_USER, &orphan).is_none());
    assert!(remaining.find_relation("other", &orphan).is_some());
    assert!(remaining.find_relation("other", &reference).is_some());
    assert_eq!(
        std::fs::read(store::catalog_path(&directory.0)).unwrap(),
        catalog_before
    );
}
