use super::*;

fn line(keys: impl IntoIterator<Item = Key>) -> io::Result<String> {
    let mut keys = keys.into_iter();
    read_keys(|| {
        keys.next()
            .ok_or_else(|| io::Error::from(io::ErrorKind::UnexpectedEof))
    })
}

#[test]
fn editing_keeps_unicode_and_spaces_without_exposing_the_password() {
    assert_eq!(
        line([
            Key::Char(' '),
            Key::Char('é'),
            Key::Char('🔑'),
            Key::Backspace,
            Key::Char('音'),
            Key::Ignored,
            Key::Char(' '),
            Key::Enter,
        ])
        .unwrap(),
        " é音 "
    );
    assert_eq!(
        line([
            Key::Char('x'),
            Key::Char('\u{15}'),
            Key::Char('y'),
            Key::Enter,
        ])
        .unwrap(),
        "y"
    );
}

#[test]
fn byte_limit_refuses_overflow_instead_of_truncating_it() {
    let keys = || std::iter::repeat_n(Key::Char('é'), MAX_PASSWORD_BYTES / 2);
    assert_eq!(
        line(keys().chain([Key::Enter])).unwrap().len(),
        MAX_PASSWORD_BYTES
    );
    let error = line(keys().chain([Key::Char('x'), Key::Enter])).unwrap_err();
    assert_eq!(error.kind(), io::ErrorKind::InvalidInput);
    assert!(!error.to_string().contains('é'));
    assert_eq!(
        line(keys().chain([Key::Char('🔑'), Key::Backspace, Key::Enter]))
            .unwrap()
            .len(),
        MAX_PASSWORD_BYTES
    );
    assert_eq!(
        line(keys().chain([Key::Char('x'), Key::Char('\u{15}'), Key::Enter])).unwrap(),
        ""
    );
}

#[test]
fn cancelled_or_incomplete_input_never_becomes_a_password() {
    for cancel in [Key::CtrlC, Key::Escape, Key::Char('\u{4}')] {
        let error = line([Key::Char('x'), cancel, Key::Enter]).unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::Interrupted);
    }
    assert_eq!(
        line([Key::Char('x')]).unwrap_err().kind(),
        io::ErrorKind::UnexpectedEof
    );
}

#[test]
fn control_characters_are_refused_instead_of_silently_changing_the_password() {
    for character in ['\0', '\t', '\u{7}'] {
        assert!(line([Key::Char(character), Key::Enter]).is_err());
    }
    assert!(line([Key::Tab, Key::Enter]).is_err());
}

#[test]
fn confirmation_preserves_exact_input_and_reports_no_secrets() {
    let secret = " a long Unicode 🔑 passphrase ";
    let mut prompts = Vec::new();
    assert_eq!(
        confirmed(|prompt| {
            prompts.push(prompt);
            Ok(secret.to_owned())
        })
        .unwrap(),
        secret
    );
    assert_eq!(prompts, ["Password: ", "Confirm password: "]);
    let mut values = [Ok(secret.to_owned()), Ok(secret.trim().to_owned())].into_iter();
    let error = confirmed(|_| values.next().unwrap()).unwrap_err();
    assert_eq!(error.kind(), io::ErrorKind::InvalidInput);
    assert!(!error.to_string().contains(secret.trim()));
}

#[test]
fn failed_first_entry_does_not_ask_for_confirmation() {
    let mut reads = 0;
    let error = confirmed(|_| {
        reads += 1;
        Err(io::Error::from(io::ErrorKind::Interrupted))
    })
    .unwrap_err();
    assert_eq!(reads, 1);
    assert_eq!(error.kind(), io::ErrorKind::Interrupted);
}

#[test]
fn redirected_input_preserves_spaces_and_accepts_full_length_lf_or_crlf_lines() {
    for ending in ["", "\n", "\r\n"] {
        let secret = "é".repeat(MAX_PASSWORD_BYTES / 2);
        assert_eq!(
            redirected(format!("{secret}{ending}").as_bytes()).unwrap(),
            secret
        );
        assert_eq!(
            redirected(format!(" a long passphrase {ending}").as_bytes()).unwrap(),
            " a long passphrase "
        );
    }
}

#[test]
fn redirected_input_refuses_multiple_lines_overflow_and_invalid_utf8() {
    for value in [
        "x".repeat(MAX_PASSWORD_BYTES + 1),
        "a long passphrase\nsecond".to_owned(),
        "a long passphrase\rsecond".to_owned(),
    ] {
        assert_eq!(
            redirected(value.as_bytes()).unwrap_err().kind(),
            io::ErrorKind::InvalidInput
        );
    }
    assert!(redirected(&[255u8][..]).is_err());
}
