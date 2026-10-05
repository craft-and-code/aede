//! Test-only DNS-SD packet construction, including compressed names.

pub(super) fn dns_name(output: &mut Vec<u8>, name: &str) {
    for label in name.split('.') {
        output.push(label.len() as u8);
        output.extend_from_slice(label.as_bytes());
    }
    output.push(0);
}

pub(super) fn packet(ip: [u8; 4]) -> Vec<u8> {
    let mut output = vec![0, 0, 0x84, 0, 0, 0, 0, 4, 0, 0, 0, 0];
    let instance = "test._googlecast._tcp.local";
    for (name, kind, data) in [
        ("_googlecast._tcp.local", 12u16, {
            let mut data = Vec::new();
            dns_name(&mut data, instance);
            data
        }),
        (instance, 33, {
            let mut data = vec![0, 0, 0, 0, 0x1f, 0x49];
            dns_name(&mut data, "test-host.local");
            data
        }),
        (instance, 16, b"\x0afn=Speaker".to_vec()),
        ("test-host.local", 1, ip.to_vec()),
    ] {
        dns_name(&mut output, name);
        output.extend_from_slice(&kind.to_be_bytes());
        output.extend_from_slice(&[0x80, 1, 0, 0, 0, 120]);
        output.extend_from_slice(&(data.len() as u16).to_be_bytes());
        output.extend_from_slice(&data);
    }
    output
}
