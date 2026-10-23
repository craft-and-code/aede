use super::*;

#[test]
fn accepted_selection_at_the_scan_budget_reaches_the_real_administrative_queue() {
    let fixture = Fixture::new();
    let mut host = Host::new(fixture.0.clone(), engine()).unwrap();
    let roots = crate::tests::selection::selection_near_limit(&fixture.0, true);
    assert!(
        roots
            .iter()
            .all(|root| !std::path::Path::new(root).is_dir())
    );
    host.configure(available_port(), roots).unwrap();
    host.start().unwrap();
    let accepted = host.scan().unwrap();
    assert!(accepted["task_id"].as_u64().unwrap() > 0);
    host.stop().unwrap();
}
