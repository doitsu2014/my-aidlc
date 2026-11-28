//! `dtech-aidlc monitor` (FN-033, IF-019): a read-only dashboard server on 127.0.0.1. The only
//! output is the stderr startup line; nothing goes to stdout. An optional `--port` fixes the port.

pub mod http;

use crate::telemetry::{hex, os_random, store_state, StoreState};
use std::net::TcpListener;
use std::path::Path;

const PORT_RANGE_MSG: &str = "Option --port requires a whole number from 1 to 65535";

/// Digits-only 1..=65535; leading zeros allowed. `u16::from_str` alone would accept `+`.
pub fn parse_port(value: &str) -> Result<u16, String> {
    let digits = value.trim_start_matches('0');
    if value.is_empty() || !value.bytes().all(|b| b.is_ascii_digit()) || digits.len() > 5 {
        return Err(PORT_RANGE_MSG.into());
    }
    match digits.parse::<u16>() {
        Ok(n) if n != 0 => Ok(n),
        _ => Err(PORT_RANGE_MSG.into()),
    }
}

/// Starts the server and never returns `Ok`: only a signal ends the process.
pub fn run(root: &Path, port: Option<&str>) -> Result<u8, String> {
    let fixed = port.map(parse_port).transpose()?;
    match store_state(root) {
        StoreState::Unsafe => return Err("Refusing unsafe .aidlc/telemetry path".into()),
        StoreState::Dir => {
            std::fs::read_dir(root.join(".aidlc").join("telemetry"))
                .map_err(|_| "Cannot read the telemetry directory".to_owned())?;
        }
        StoreState::Absent => {}
    }
    let mut bytes = [0u8; 16];
    os_random(&mut bytes).map_err(|_| "Cannot create a monitor token".to_owned())?;
    let token = hex(&bytes);
    let listener =
        TcpListener::bind(("127.0.0.1", fixed.unwrap_or(0))).map_err(|_| match fixed {
            Some(n) => format!("Cannot bind 127.0.0.1:{n} (port in use or not permitted)"),
            None => "Cannot bind a loopback port".to_owned(),
        })?;
    let port = listener
        .local_addr()
        .map_err(|_| "Cannot bind a loopback port".to_owned())?
        .port();
    eprintln!("monitor: http://127.0.0.1:{port}/#token={token}  (Ctrl+C to stop)");
    http::serve(listener, root.to_path_buf(), token)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_port_accepts_the_range_and_leading_zeros() {
        for (v, n) in [
            ("1", 1),
            ("80", 80),
            ("8787", 8787),
            ("08787", 8787),
            ("65535", 65535),
            ("0065535", 65535),
        ] {
            assert_eq!(parse_port(v), Ok(n), "{v}");
        }
    }

    #[test]
    fn parse_port_refuses_everything_else() {
        for v in [
            "0",
            "00",
            "65536",
            "99999999999999999999",
            "abc",
            "80a",
            "8 0",
            "0x50",
            "1e3",
            "+8080",
            "-8080",
            "8080.0",
            "",
            "\u{668}\u{660}",
        ] {
            assert_eq!(
                parse_port(v),
                Err("Option --port requires a whole number from 1 to 65535".to_owned()),
                "{v:?}"
            );
        }
    }
}
