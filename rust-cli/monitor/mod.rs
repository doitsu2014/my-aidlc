//! `dtech-aidlc monitor` (FN-033, IF-019): a read-only dashboard server on 127.0.0.1. The only
//! output is the stderr startup line; nothing goes to stdout.

pub mod http;

use crate::telemetry::{hex, os_random, store_state, StoreState};
use std::net::TcpListener;
use std::path::Path;

/// Starts the server and never returns `Ok`: only a signal ends the process.
pub fn run(root: &Path) -> Result<u8, String> {
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
    let listener = TcpListener::bind(("127.0.0.1", 0))
        .map_err(|_| "Cannot bind a loopback port".to_owned())?;
    let port = listener
        .local_addr()
        .map_err(|_| "Cannot bind a loopback port".to_owned())?
        .port();
    eprintln!("monitor: http://127.0.0.1:{port}/#token={token}  (Ctrl+C to stop)");
    http::serve(listener, root.to_path_buf(), token)
}
