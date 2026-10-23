use super::*;

const SCAN_BODY_LIMIT: usize = 16 * 1024;

fn worst_case_body(roots: &[String]) -> Value {
    json!({"folders":roots,"replace":true,"threads":1,"unavailable_folders":roots})
}

pub(super) fn selection_near_limit(root: &std::path::Path, escaped: bool) -> Vec<String> {
    let mut roots: Vec<_> = (0..4)
        .map(|index| {
            let label = if escaped { "é\"\\music" } else { "music" };
            root.join(format!("{index}-{label}-{}", "a".repeat(1800)))
                .display()
                .to_string()
        })
        .collect();
    while worst_case_body(&roots).to_string().len() + 2 <= SCAN_BODY_LIMIT {
        roots.last_mut().unwrap().push('a');
    }
    assert!(roots.iter().all(|path| path.len() <= 4096));
    let size = worst_case_body(&roots).to_string().len();
    assert!(size <= SCAN_BODY_LIMIT && size > SCAN_BODY_LIMIT - 2);
    roots
}

fn oversized_selection(root: &std::path::Path) -> Vec<String> {
    (0..8)
        .map(|index| {
            root.join(format!("{index}-{}", "a".repeat(2000)))
                .display()
                .to_string()
        })
        .collect()
}

#[test]
fn selections_at_the_serialized_scan_budget_are_saved_including_escaped_unicode_paths() {
    for escaped in [false, true] {
        let fixture = Fixture::new();
        let mut host = fixture.host();
        let roots = selection_near_limit(&fixture.0, escaped);
        host.configure(18877, roots.clone()).unwrap();
        assert_eq!(read(&fixture.0).unwrap().roots, roots);
    }
}

#[test]
fn selections_above_the_serialized_scan_budget_preserve_the_previous_profile_and_identity() {
    for escaped in [false, true] {
        let fixture = Fixture::new();
        let mut host = fixture.host();
        let mut config = read(&fixture.0).unwrap();
        config.identity = Some(configuration::Identity::generate().unwrap());
        save(&fixture.0, &config).unwrap();
        let before = fs::read(fixture.0.join("server.json")).unwrap();
        let mut roots = selection_near_limit(&fixture.0, escaped);
        roots.last_mut().unwrap().push('a');
        assert!(worst_case_body(&roots).to_string().len() > SCAN_BODY_LIMIT);
        let error = host
            .configure(18877, roots)
            .err()
            .expect("oversized selection accepted");
        assert!(error.contains("Réduisez"), "missing actionable refusal");
        assert_eq!(fs::read(fixture.0.join("server.json")).unwrap(), before);
    }
}

#[test]
fn legacy_large_profiles_remain_readable_and_can_reduce_their_selected_roots() {
    let fixture = Fixture::new();
    let host = fixture.host();
    let mut legacy = read(&fixture.0).unwrap();
    legacy.roots = oversized_selection(&fixture.0);
    legacy.identity = Some(configuration::Identity::generate().unwrap());
    save(&fixture.0, &legacy).unwrap();
    assert!(worst_case_body(&legacy.roots).to_string().len() > SCAN_BODY_LIMIT);
    let password = host.password().unwrap();
    let before = fs::read(fixture.0.join("server.json")).unwrap();
    assert!(before.len() < 128 * 1024);
    drop(host);
    let mut reopened = fixture.host();
    assert_eq!(reopened.status().unwrap().roots.len(), legacy.roots.len());
    assert_eq!(reopened.password().unwrap(), password);
    assert_eq!(fs::read(fixture.0.join("server.json")).unwrap(), before);
    let reduced = vec![legacy.roots[0].clone()];
    reopened.configure(18877, reduced.clone()).unwrap();
    assert_eq!(read(&fixture.0).unwrap().roots, reduced);
    assert_eq!(reopened.password().unwrap(), password);
}

#[test]
fn legacy_large_selection_gets_an_actionable_scan_refusal_without_rewriting_it() {
    let fixture = Fixture::new();
    let mut host = fixture.host();
    let mut legacy = read(&fixture.0).unwrap();
    legacy.roots = oversized_selection(&fixture.0);
    save(&fixture.0, &legacy).unwrap();
    let before = fs::read(fixture.0.join("server.json")).unwrap();
    let error = host.scan().unwrap_err();
    assert!(error.contains("Réduisez"), "missing actionable refusal");
    assert_eq!(fs::read(fixture.0.join("server.json")).unwrap(), before);
}
