//! An entry said on the systemd journal's native socket, whatever the process's stderr is.

use std::os::unix::net::UnixDatagram;

const SOCKET: &str = "/run/systemd/journal/socket";

/// Say an entry of `fields` (name, value) to the journal.
///
/// # Errors
/// The socket's: no journal listening, or a datagram refused.
pub(crate) fn say(fields: &[(&str, &str)]) -> std::io::Result<()> {
    UnixDatagram::unbound()?
        .send_to(&entry(fields), SOCKET)
        .map(|_| ())
}

/// Each field in the protocol's length-prefixed form, which holds any value.
fn entry(fields: &[(&str, &str)]) -> Vec<u8> {
    let mut entry = Vec::new();
    for (name, value) in fields {
        entry.extend_from_slice(name.as_bytes());
        entry.push(b'\n');
        entry.extend_from_slice(&(value.len() as u64).to_le_bytes());
        entry.extend_from_slice(value.as_bytes());
        entry.push(b'\n');
    }
    entry
}

#[cfg(test)]
mod tests {
    #[test]
    fn each_field_is_its_name_then_its_length_then_its_value() {
        let said = [
            &b"MESSAGE\n"[..],
            &3u64.to_le_bytes(),
            b"a\nb\nSUZATARY_MS\n",
            &2u64.to_le_bytes(),
            b"12\n",
        ]
        .concat();
        assert_eq!(
            super::entry(&[("MESSAGE", "a\nb"), ("SUZATARY_MS", "12")]),
            said
        );
    }
}
