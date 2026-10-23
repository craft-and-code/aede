use super::read_stop_ack;
use std::io::{BufReader, Cursor, Read};

#[path = "process_output_test_support.rs"]
mod support;
use support::ReadFailure;

const ACK: &[u8] = b"{\"desktopStopped\":1}\n";

#[test]
fn informational_stdout_does_not_hide_a_later_stop_acknowledgement() {
    let output = b"Device media: 127.0.0.1:1234\nSlimProto: waiting for selected player\n{\"desktopStopped\":1}\n";
    assert!(read_stop_ack(Cursor::new(output), true).unwrap());
}

#[test]
fn informational_stdout_and_eof_do_not_confirm_stop() {
    for output in [
        b"".as_slice(),
        b"Device media: 127.0.0.1:1234\n",
        b"diagnostic",
    ] {
        assert!(!read_stop_ack(Cursor::new(output), true).unwrap());
    }
}

#[test]
fn stop_acknowledgement_requires_the_complete_canonical_frame() {
    for output in [
        b"{\"desktopStopped\":1}".as_slice(),
        b"{\"desktopStopped\":1}\r\n",
        b" {\"desktopStopped\":1}\n",
        b"{\"desktopStopped\":1} \n",
        b"{\"desktopStopped\": 1}\n",
        b"{\"desktopStopped\":1,\"extra\":true}\n",
        b"{\"desktopStopped\":0,\"desktopStopped\":1}\n",
        b"{\"desktopStopped\":1,\"desktopStopped\":1}\n",
        b"{\"desktopStopped\":true}\n",
        b"[{\"desktopStopped\":1}]\n",
        b"{\"desktopStopped\":1}{}\n",
        b"diagnostic {\"desktopStopped\":1}\n",
        b"{\"desktopStopped\":1}\0\n",
    ] {
        assert!(!read_stop_ack(Cursor::new(output), true).unwrap());
    }
}

#[test]
fn invalid_frames_do_not_hide_a_later_complete_stop_acknowledgement() {
    let output =
        b"{\"desktopStopped\":0}\n{\"desktopStopped\":1,\"extra\":true}\n{\"desktopStopped\":1}\n";
    assert!(read_stop_ack(Cursor::new(output), true).unwrap());
}

#[test]
fn fragmented_reads_preserve_complete_acknowledgement_frames() {
    for capacity in [1, 2, 7, ACK.len() - 1, ACK.len(), 4096] {
        let reader = BufReader::with_capacity(capacity, Cursor::new(ACK));
        assert!(read_stop_ack(reader, true).unwrap());
    }
}

#[test]
fn oversized_diagnostic_suffixes_cannot_become_stop_acknowledgements() {
    for size in [4096, 4097, 4098, 8193, 8194, 8195, 1_048_576] {
        let prefix = std::io::repeat(b'x').take(size);
        let output = prefix.chain(Cursor::new(ACK));
        assert!(!read_stop_ack(BufReader::new(output), true).unwrap());
    }
}

#[test]
fn oversized_diagnostics_are_drained_before_the_next_complete_frame() {
    for size in [4096, 4097, 4098, 8193, 8194, 8195, 1_048_576] {
        let prefix = std::io::repeat(b'x').take(size);
        let output = prefix.chain(Cursor::new(b"\n")).chain(Cursor::new(ACK));
        assert!(read_stop_ack(BufReader::new(output), true).unwrap());
    }
}

#[test]
fn a_truncated_previous_line_cannot_supply_an_acknowledgement_suffix() {
    assert!(!read_stop_ack(Cursor::new(ACK), false).unwrap());
    let output = Cursor::new(ACK).chain(Cursor::new(ACK));
    assert!(read_stop_ack(BufReader::new(output), false).unwrap());
}

#[test]
fn malformed_utf8_diagnostics_do_not_hide_a_complete_ascii_frame() {
    let output = b"\xff\xfe diagnostic\n{\"desktopStopped\":1}\n";
    assert!(read_stop_ack(Cursor::new(output), true).unwrap());
    let output = b"\xff{\"desktopStopped\":1}\n";
    assert!(!read_stop_ack(Cursor::new(output), true).unwrap());
}

#[test]
fn stdout_read_errors_refuse_confirmation_before_or_after_the_frame() {
    for output in [b"".as_slice(), b"diagnostic\n", ACK] {
        let reader = BufReader::with_capacity(1, ReadFailure::after(output));
        assert!(read_stop_ack(reader, true).is_err());
    }
}

#[test]
fn diagnostics_after_a_complete_acknowledgement_are_still_drained() {
    let output = Cursor::new(ACK).chain(std::io::repeat(b'x').take(1_048_576));
    let mut reader = BufReader::new(output);
    assert!(read_stop_ack(&mut reader, true).unwrap());
    let mut remaining = Vec::new();
    reader.read_to_end(&mut remaining).unwrap();
    assert!(remaining.is_empty());
}
