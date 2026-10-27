use super::*;

fn payload(data: &[u8], visit: impl FnOnce(&mut Reader<'_>, Atom)) {
    let source = crate::test_support::Wave::new(&[0.0], 1);
    std::fs::write(&source.0, data).unwrap();
    let mut file = File::open(&source.0).unwrap();
    let mut reader = Reader::new(&mut file).unwrap();
    visit(
        &mut reader,
        Atom {
            kind: *b"test",
            body: 0,
            end: data.len() as u64,
        },
    );
}

#[test]
fn aac_box_extents_and_truncated_extended_headers_are_errors() {
    for bytes in [
        vec![0; 1],
        vec![0; 7],
        [16u32.to_be_bytes().as_slice(), b"free"].concat(),
        [1u32.to_be_bytes().as_slice(), b"free"].concat(),
        [
            1u32.to_be_bytes().as_slice(),
            b"free",
            &u64::MAX.to_be_bytes(),
        ]
        .concat(),
        [7u32.to_be_bytes().as_slice(), b"free"].concat(),
    ] {
        payload(&bytes, |r, a| assert!(r.children(a.body, a.end).is_err()));
    }
    let bytes = [0u32.to_be_bytes().as_slice(), b"free"].concat();
    payload(&bytes, |r, a| {
        assert_eq!(r.children(a.body, a.end).unwrap().len(), 1)
    });
}

#[test]
fn aac_timing_requires_positive_clocks_normal_edits_and_bounded_samples() {
    for version in [0, 1] {
        let mut data = vec![0u8; if version == 0 { 20 } else { 32 }];
        data[0] = version;
        let at = if version == 0 { 12 } else { 20 };
        data[at..at + 4].copy_from_slice(&48000u32.to_be_bytes());
        if version == 0 {
            data[16..20].copy_from_slice(&49037u32.to_be_bytes());
        } else {
            data[24..32].copy_from_slice(&49037u64.to_be_bytes());
        }
        payload(&data, |r, a| {
            assert_eq!(clock(r, a).unwrap(), (48000, 49037))
        });
        for end in 0..data.len() {
            payload(&data[..end], |r, a| assert!(clock(r, a).is_err()));
        }
        data[1] = 1;
        payload(&data, |r, a| assert!(clock(r, a).is_err()));
    }
    for version in [0, 1] {
        let mut data = vec![0u8; if version == 0 { 20 } else { 28 }];
        data[0] = version;
        data[4..8].copy_from_slice(&1u32.to_be_bytes());
        let at = if version == 0 { 16 } else { 24 };
        data[at..at + 4].copy_from_slice(&0x10000u32.to_be_bytes());
        if version == 0 {
            data[8..12].copy_from_slice(&48013u32.to_be_bytes());
            data[12..16].copy_from_slice(&1024u32.to_be_bytes());
        } else {
            data[8..16].copy_from_slice(&48013u64.to_be_bytes());
            data[16..24].copy_from_slice(&1024u64.to_be_bytes());
        }
        payload(&data, |r, a| {
            assert_eq!(single_edit(r, a).unwrap(), (48013, 1024))
        });
        for end in 0..data.len() {
            payload(&data[..end], |r, a| assert!(single_edit(r, a).is_err()));
        }
    }
    for (count, delta) in [(0, 1024), (2, 1023), (1, 0), (1, 1025)] {
        let mut data = vec![0; 4];
        for n in [1u32, count, delta] {
            data.extend(n.to_be_bytes());
        }
        payload(&data, |r, a| assert!(samples(r, a).is_err()));
    }
    let mut huge = vec![0; 4];
    huge.extend(4097u32.to_be_bytes());
    payload(&huge, |r, a| assert!(samples(r, a).is_err()));
}
