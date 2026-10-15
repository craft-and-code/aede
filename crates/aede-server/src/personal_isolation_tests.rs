use super::*;
use crate::accounts_test_support::Fixture;

#[test]
fn sign_out_during_a_personal_update_refuses_publication_and_keeps_both_owners_bytes() {
    let fixture = Fixture::new();
    let accounts = fixture.accounts();
    let alice = accounts.find("alice").unwrap();
    let bob = accounts.find("bob").unwrap();
    let (principal, _) = auth::start_session(&fixture.0, &accounts, alice).unwrap();
    let owner = PersonalOwner {
        id: alice.id.clone(),
        principal: Some(principal.clone()),
    };
    let catalog = fixture.0.catalog.try_read().unwrap().clone().unwrap();
    let target = EntityRef::of(&catalog, EntityKind::Track, 0).unwrap();
    let mut data = UserData::default();
    data.entry(&alice.id, &target, 1).note = Some("Alice original".into());
    data.entry(&bob.id, &target, 1).note = Some("Bob untouched".into());
    let path = user::user_path(&fixture.0.data_dir);
    user::save(&data, &path).unwrap();
    let before = std::fs::read(&path).unwrap();
    let result = locked_update(&fixture.0.data_dir, &fixture.0, &owner, |_, data| {
        data.entry(&owner.id, &target, 2).note = Some("must not publish".into());
        auth::end_session(&fixture.0, &principal)?;
        Ok(())
    });
    assert_eq!(result.err().unwrap().status, StatusCode::UNAUTHORIZED);
    assert_eq!(std::fs::read(&path).unwrap(), before);
}

#[test]
fn a_prepared_private_projection_is_not_admitted_after_its_session_signs_out() {
    let fixture = Fixture::new();
    let accounts = fixture.accounts();
    let alice = accounts.find("alice").unwrap();
    let (principal, _) = auth::start_session(&fixture.0, &accounts, alice).unwrap();
    let owner = PersonalOwner {
        id: alice.id.clone(),
        principal: Some(principal.clone()),
    };
    let catalog = fixture.0.catalog.try_read().unwrap().clone().unwrap();
    let reference = EntityRef::of(&catalog, EntityKind::Track, 0).unwrap();
    let mut data = UserData::default();
    data.entry(&owner.id, &reference, 1).note = Some("Alice private".into());
    owner.recheck(&fixture.0).unwrap();
    let prepared = Json(AnnotationResponse {
        reference: reference.to_token(),
        annotation: data.find(&owner.id, &reference).map(annotation_view),
    });
    auth::end_session(&fixture.0, &principal).unwrap();
    assert_eq!(
        owner.admit(&fixture.0, prepared).err().unwrap().status,
        StatusCode::UNAUTHORIZED
    );
}
