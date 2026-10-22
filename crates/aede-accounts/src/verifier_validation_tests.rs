use super::*;

use super::test_support::PASSWORD;

const SALT: &str = "YWFhYWFhYWFhYWFhYWFhYQ";
const CREDENTIAL_FIELDS: [(&str, &str); 2] =
    [("account", "password_hash"), ("api_keys", "secret_hash")];

fn credential_document() -> Json {
    serde_json::json!({
        "format_version": 1,
        "epoch": "e".repeat(64),
        "account": [{
            "id": "local",
            "username": "operator",
            "role": "admin",
            "enabled": true,
            "created_at": 10,
            "updated_at": 10,
            "revision": 1,
            "password_hash": DUMMY_HASH,
        }],
        "api_keys": [{
            "id": "1".repeat(64),
            "label": "test client",
            "owner": "local",
            "created_at": 10,
            "revision": 1,
            "secret_hash": DUMMY_HASH,
        }],
    })
}

fn assert_verifier_rejected(collection: &str, field: &str, hash: &str) {
    let mut document = credential_document();
    document[collection][0][field] = hash.into();
    assert!(
        from_json(&document).is_err(),
        "accepted malformed {field}: {hash}"
    );
}

fn assert_extreme_lanes_rejected(collection: &str, field: &str) {
    for lanes in [
        "4294967295",
        "536870912",
        "536870911",
        "16777216",
        "16777215",
        "4294967296",
        "0",
        "2",
    ] {
        let hash = DUMMY_HASH.replace("p=1", &format!("p={lanes}"));
        assert_verifier_rejected(collection, field, &hash);
    }
}

fn assert_invalid_salts_rejected(collection: &str, field: &str) {
    for salt in [
        "YWFhYWFhYWFhYWFhYWFhY.",
        "YWFhYWFhYWFhYWFhYWFhY-",
        "YWFhYWFhYWFhYWFhYWFhYR",
    ] {
        let hash = DUMMY_HASH.replace(SALT, salt);
        assert_verifier_rejected(collection, field, &hash);
    }
}

#[test]
fn password_verifiers_with_extreme_lanes_are_rejected_without_panicking() {
    assert_extreme_lanes_rejected("account", "password_hash");
}

#[test]
fn api_key_verifiers_with_extreme_lanes_are_rejected_without_panicking() {
    assert_extreme_lanes_rejected("api_keys", "secret_hash");
}

#[test]
fn password_verifiers_with_undecodable_or_noncanonical_salts_are_rejected() {
    assert_invalid_salts_rejected("account", "password_hash");
}

#[test]
fn api_key_verifiers_with_undecodable_or_noncanonical_salts_are_rejected() {
    assert_invalid_salts_rejected("api_keys", "secret_hash");
}

#[test]
fn malformed_verifiers_are_rejected_in_both_credential_fields() {
    let output = "A".repeat(43);
    let variants = [
        String::new(),
        "x".repeat(257),
        DUMMY_HASH.replace("argon2id", "argon2i"),
        DUMMY_HASH.replace("v=19", "v=16"),
        DUMMY_HASH.replace("m=19456", "m=19455"),
        DUMMY_HASH.replace("m=19456", "m=4294967295"),
        DUMMY_HASH.replace("t=2", "t=0"),
        DUMMY_HASH.replace("t=2", "t=3"),
        DUMMY_HASH.replace("p=1", "p=01"),
        DUMMY_HASH.replace("p=1", "p=1,p=1"),
        DUMMY_HASH.replace("p=1", "unknown=1"),
        DUMMY_HASH.replace("m=19456,t=2,p=1", "m=19456,t=2"),
        DUMMY_HASH.replace(SALT, ""),
        DUMMY_HASH.replace(SALT, &"A".repeat(21)),
        DUMMY_HASH.replace(SALT, &"A".repeat(23)),
        DUMMY_HASH.replace(&output, &"A".repeat(42)),
        DUMMY_HASH.replace(&output, &"A".repeat(44)),
        DUMMY_HASH.replace(&format!("${output}"), ""),
    ];
    for (collection, field) in CREDENTIAL_FIELDS {
        for hash in &variants {
            assert_verifier_rejected(collection, field, hash);
        }
    }
}

#[test]
fn legacy_effective_policy_verifiers_remain_loadable() {
    for parameters in [
        "m=19456,t=2,p=1",
        "p=1,m=19456,t=2",
        "m=19456,t=2,data=YQ",
        "t=2,p=1,data=YQ",
        "m=19456,p=1,keyid=YQ",
        "m=19456,keyid=YQ,data=YQ",
    ] {
        let hash = DUMMY_HASH.replace("m=19456,t=2,p=1", parameters);
        let mut document = credential_document();
        for (collection, field) in CREDENTIAL_FIELDS {
            document[collection][0][field] = hash.clone().into();
        }
        let loaded = from_json(&document).expect("compatible fixed-policy verifier");
        assert_eq!(to_json(&loaded), document);
    }
}

#[test]
fn generated_verifiers_authenticate_after_store_round_trip() {
    let secret = "2".repeat(64);
    let password_salt = SaltString::encode_b64(&[255; 16]).unwrap();
    let key_salt = SaltString::encode_b64(&[251; 16]).unwrap();
    let mut document = credential_document();
    for (field, input, salt) in [
        ("password_hash", PASSWORD, &password_salt),
        ("secret_hash", secret.as_str(), &key_salt),
    ] {
        let hash = hasher()
            .unwrap()
            .hash_password(input.as_bytes(), salt)
            .unwrap()
            .to_string();
        let collection = if field == "password_hash" {
            "account"
        } else {
            "api_keys"
        };
        document[collection][0][field] = hash.replace("m=19456,t=2,p=1", "t=2,p=1,m=19456").into();
    }
    let loaded = from_json(&document).expect("generated fixed-policy verifiers");
    let reloaded = from_json(&to_json(&loaded)).unwrap();
    assert_eq!(loaded, reloaded);
    assert!(reloaded.authenticate("OPERATOR", PASSWORD).is_some());
    assert!(
        reloaded
            .authenticate_api_key(&format!("{}.{}", "1".repeat(64), secret))
            .is_some()
    );
}
