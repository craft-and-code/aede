use super::*;
use serde_json::json;

use crate::test_support::heartbeat_message as message;

#[test]
fn envelopes_preserve_routes_and_payload_without_accepting_ambiguous_fields() {
    let input = encode(&message()).unwrap();
    let golden = b"\x08\x00\x12\x08sender-1\x1a\x0areceiver-0\x22\x27urn:x-cast:com.google.cast.tp.heartbeat\x28\x00\x32\x0f{\"type\":\"PING\"}";
    assert_eq!(input, golden, "independent Cast V2 protobuf wire vector");
    let parsed = decode(&input).unwrap();
    assert_eq!(parsed.source, "sender-1");
    assert_eq!(parsed.destination, "receiver-0");
    assert_eq!(parsed.namespace, message().namespace);
    assert_eq!(parsed.payload, json!({"type":"PING"}));
    let mut duplicate = input.clone();
    duplicate.extend_from_slice(&[8, 0]);
    assert!(decode(&duplicate).unwrap_err().contains("duplicate"));
    for cut in 0..input.len() {
        assert!(decode(&input[..cut]).is_err(), "{cut}");
    }
    let mut unknown = input;
    unknown.extend_from_slice(&[0x80, 1, 5]);
    assert!(decode(&unknown).is_ok());
}

#[test]
fn envelope_lengths_binary_payloads_utf8_and_varints_are_bounded() {
    for input in [
        vec![0],
        vec![8, 1],
        vec![8, 0, 0x12, 0xff, 0xff, 0xff, 0xff, 0x7f],
        vec![255; 12],
        vec![0; MAX_FRAME + 1],
    ] {
        assert!(decode(&input).is_err());
    }
    let mut binary = encode(&message()).unwrap();
    let at = binary.windows(2).position(|pair| pair == [40, 0]).unwrap();
    binary[at + 1] = 1;
    assert!(decode(&binary).is_err());
    let mut invalid = message();
    invalid.source = "bad\nroute".into();
    assert!(encode(&invalid).is_err());
    invalid = message();
    invalid.payload = json!({"large":"x".repeat(MAX_FRAME)});
    assert!(encode(&invalid).is_err());
    invalid = message();
    invalid.payload = json!([]);
    assert!(encode(&invalid).is_err());
    assert!(text(&[0xff], 128).is_err());
    let mut offset = 0;
    assert!(varint(&[0xff; 10], &mut offset).is_err());
}

#[tokio::test]
async fn fragmented_frames_are_reassembled_and_oversized_prefixes_are_refused() {
    let (mut tx, mut rx) = tokio::io::duplex(128);
    let bytes = encode(&message()).unwrap();
    let send = tokio::spawn(async move {
        tx.write_u32(bytes.len() as u32).await.unwrap();
        for byte in bytes {
            tx.write_all(&[byte]).await.unwrap();
            tokio::task::yield_now().await;
        }
        tx.write_u32(u32::MAX).await.unwrap();
    });
    assert_eq!(read(&mut rx).await.unwrap().payload, json!({"type":"PING"}));
    assert!(read(&mut rx).await.unwrap_err().contains("limit"));
    send.await.unwrap();
}
