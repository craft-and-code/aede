#![cfg(feature = "engine-tests")]

#[path = "support/legacy_selection_recovery.rs"]
mod support;

use serde_json::{Value, json};
use std::{fs, path::PathBuf};
use support::{Fixture, completed_scan};

#[test]
fn running_legacy_profile_recovers_through_single_removals_and_a_real_scan() {
    let fixture = Fixture::new();
    let mut roots = fixture.seed_legacy_profile();
    let mut host = fixture.host();
    host.start().unwrap();
    let initial = fixture.configuration();
    let identity = initial["identity"].clone();
    let password = host.password().unwrap();
    let port = initial["port"].as_u64().unwrap() as u16;
    let mut oversized_reductions = 0;

    while roots.len() > 1 {
        let removed = roots.last().unwrap().clone();
        roots.retain(|path| path != &removed);
        let status = serde_json::to_value(host.configure(port, roots.clone()).unwrap()).unwrap();
        assert_eq!(status["running"], true);
        let selected: Vec<PathBuf> = status["roots"]
            .as_array()
            .unwrap()
            .iter()
            .map(|root| PathBuf::from(root["path"].as_str().unwrap()))
            .collect();
        assert_eq!(
            selected,
            roots.iter().map(PathBuf::from).collect::<Vec<_>>()
        );
        let bytes = fs::read(fixture.profile()).unwrap();
        let saved: Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(saved["roots"], json!(roots));
        assert_eq!(saved["identity"], identity);
        assert_eq!(host.password().unwrap(), password);

        let body = json!({"folders":roots,"replace":true,"threads":1,"unavailable_folders":roots});
        if body.to_string().len() > 16 * 1024 {
            oversized_reductions += 1;
            let refusal = host.scan().unwrap_err();
            assert!(refusal.contains("Réduisez"), "{refusal}");
            assert_eq!(fs::read(fixture.profile()).unwrap(), bytes);
            let other_port = if port == u16::MAX { port - 1 } else { port + 1 };
            assert!(
                host.configure(other_port, roots[..roots.len() - 1].to_vec())
                    .is_err()
            );
            assert_eq!(fs::read(fixture.profile()).unwrap(), bytes);
            assert_eq!(
                serde_json::to_value(host.status().unwrap()).unwrap()["running"],
                true
            );
        }
    }
    assert!(oversized_reductions >= 3);
    completed_scan(&mut host);
    let catalog = fixture.catalog();
    let files = catalog["file"].as_array().unwrap();
    assert_eq!(files.len(), 1);
    assert_eq!(
        std::path::Path::new(files[0]["path"].as_str().unwrap()),
        fixture.0.join("music").join("test.wav")
    );
    assert_eq!(catalog["track"].as_array().unwrap().len(), 1);
    assert_eq!(fixture.configuration()["identity"], identity);
    assert_eq!(host.password().unwrap(), password);
    host.stop().unwrap();
}
