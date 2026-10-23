use super::*;

fn legacy_roots(root: &std::path::Path) -> Vec<String> {
    (0..8)
        .map(|index| {
            let mut path = root.join(format!("absent-volume-{index}"));
            for component in 0..17 {
                path = path.join(format!("{component}-{}", "a".repeat(120)));
            }
            path.to_str().unwrap().to_owned()
        })
        .collect()
}

fn oversized_profile(fixture: &Fixture) -> (Host, Vec<String>, Value) {
    let host = fixture.host();
    let mut configuration = read(&fixture.0).unwrap();
    configuration.port = 18877;
    configuration.roots = legacy_roots(&fixture.0);
    configuration.identity = Some(configuration::Identity::generate().unwrap());
    save(&fixture.0, &configuration).unwrap();
    let roots = configuration.roots.clone();
    assert!(scan_payload(&roots).is_err());
    let saved = serde_json::to_value(configuration).unwrap();
    (host, roots, saved)
}

#[test]
fn legacy_selection_accepts_each_single_folder_removal_until_scannable() {
    let fixture = Fixture::new();
    let (mut host, mut roots, original) = oversized_profile(&fixture);
    let password = host.password().unwrap();
    let mut oversized_reductions = 0;
    while let Some(removed) = roots.last().cloned() {
        roots.retain(|path| path != &removed);
        let still_oversized = scan_payload(&roots).is_err();
        oversized_reductions += usize::from(still_oversized);
        let status = host
            .configure(18877, roots.clone())
            .unwrap_or_else(|error| {
                panic!("single removal to {} roots refused: {error}", roots.len())
            });
        assert!(!status.running);
        let selected: Vec<_> = status
            .roots
            .iter()
            .map(|root| std::path::Path::new(&root.path))
            .collect();
        let expected: Vec<_> = roots.iter().map(std::path::Path::new).collect();
        assert_eq!(selected, expected);
        let bytes = fs::read(fixture.0.join("server.json")).unwrap();
        let mut expected = original.clone();
        expected["roots"] = json!(roots);
        assert_eq!(serde_json::from_slice::<Value>(&bytes).unwrap(), expected);
        assert_eq!(host.password().unwrap(), password);
        if still_oversized {
            assert!(host.scan().unwrap_err().contains("Réduisez"));
            assert_eq!(fs::read(fixture.0.join("server.json")).unwrap(), bytes);
        } else if !roots.is_empty() {
            assert_eq!(host.scan().unwrap_err(), "Le serveur est arrêté.");
        }
        drop(host);
        host = fixture.host();
        assert_eq!(host.status().unwrap().roots.len(), roots.len());
        assert_eq!(host.password().unwrap(), password);
    }
    assert!(oversized_reductions >= 4);
    assert!(read(&fixture.0).unwrap().roots.is_empty());
}

#[test]
fn oversized_legacy_recovery_refuses_edits_that_are_not_strict_root_removals() {
    let fixture = Fixture::new();
    let (mut host, roots, _) = oversized_profile(&fixture);
    let before = fs::read(fixture.0.join("server.json")).unwrap();
    let mut added = roots.clone();
    added.push(fixture.0.join("new-root").to_str().unwrap().into());
    let mut replaced = roots.clone();
    replaced[0] = fixture.0.join("replacement").to_str().unwrap().into();
    let mut fewer_with_addition = roots[..6].to_vec();
    fewer_with_addition.push(fixture.0.join("new-root").to_str().unwrap().into());
    let mut shorter_spelling = roots[..7].to_vec();
    shorter_spelling[0] = std::path::Path::new(&roots[0])
        .parent()
        .unwrap()
        .to_str()
        .unwrap()
        .into();
    let mut disguised = roots[..7].to_vec();
    disguised[0] = std::path::Path::new(&roots[0])
        .join("child")
        .join("..")
        .to_str()
        .unwrap()
        .into();
    let mut reordered = roots.clone();
    reordered.reverse();
    for candidate in [
        roots,
        added,
        replaced,
        fewer_with_addition,
        shorter_spelling,
        disguised,
        reordered,
    ] {
        assert!(scan_payload(&candidate).is_err());
        assert!(host.configure(18877, candidate).is_err());
        assert_eq!(fs::read(fixture.0.join("server.json")).unwrap(), before);
    }
}

#[test]
fn oversized_legacy_recovery_keeps_all_configuration_validation() {
    let fixture = Fixture::new();
    let (mut host, roots, _) = oversized_profile(&fixture);
    let before = fs::read(fixture.0.join("server.json")).unwrap();
    let subset = roots[..7].to_vec();
    let mut relative = subset.clone();
    relative[0] = "relative".into();
    let mut control = subset.clone();
    control[0].push('\n');
    let mut overlong = subset.clone();
    overlong[0].push_str(&"x".repeat(4097));
    let mut duplicate = subset.clone();
    duplicate[1] = duplicate[0].clone();
    for (port, candidate) in [
        (0, subset),
        (18877, relative),
        (18877, control),
        (18877, overlong),
        (18877, duplicate),
        (18877, vec![roots[0].clone(); 65]),
    ] {
        assert!(host.configure(port, candidate).is_err());
        assert_eq!(fs::read(fixture.0.join("server.json")).unwrap(), before);
    }
}

#[test]
fn legacy_recovery_never_repairs_or_rewrites_invalid_saved_data() {
    let fixture = Fixture::new();
    let (mut host, roots, original) = oversized_profile(&fixture);
    let subset = roots[..7].to_vec();
    let path = fixture.0.join("server.json");
    let mut invalid_identity = original.clone();
    invalid_identity["identity"]["password"] = json!("invalid");
    let mut invalid_path = original.clone();
    invalid_path["roots"][0] = json!("relative");
    let mut invalid_port = original;
    invalid_port["port"] = json!(0);
    for invalid in [invalid_identity, invalid_path, invalid_port] {
        let bytes = serde_json::to_vec(&invalid).unwrap();
        fs::write(&path, &bytes).unwrap();
        assert!(host.configure(18877, subset.clone()).is_err());
        assert_eq!(fs::read(&path).unwrap(), bytes);
    }
}
