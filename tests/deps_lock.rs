//! TS-031: concurrent `deps add` processes keep every edge (FN-037, AC-027).

use serde_json::Value;
use std::path::Path;
use std::process::Command;
use std::sync::{Arc, Barrier};

fn run(root: &Path, args: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_dtech-aidlc"))
        .args(args)
        .arg("--project")
        .arg(root)
        .env_remove("DTECH_AIDLC_HOME")
        .output()
        .expect("run dtech-aidlc")
}

/// Removes the temporary project even when an assertion panics.
struct Cleanup(std::path::PathBuf);
impl Drop for Cleanup {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[test]
fn ts031_concurrent_deps_add_keeps_three_edges() {
    let dir = std::env::temp_dir().join(format!("aidlc-deps-lock-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let _cleanup = Cleanup(dir.clone());
    for args in [
        &["onboard", "--harness", "claude", "--scope", "solo"][..],
        &["orchestrate", "next", "--intent", "Build a sample feature"][..],
    ] {
        let out = run(&dir, args);
        assert!(
            out.status.success(),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
    }
    let barrier = Arc::new(Barrier::new(3));
    let handles: Vec<_> = ["e1", "e2", "e3"]
        .into_iter()
        .map(|dep| {
            let (dir, barrier) = (dir.clone(), barrier.clone());
            std::thread::spawn(move || {
                barrier.wait();
                run(&dir, &["deps", "add", "x", dep])
            })
        })
        .collect();
    for handle in handles {
        let out = handle.join().unwrap();
        assert!(
            out.status.success(),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
    }
    let out = run(&dir, &["deps", "list"]);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let rows: Value = serde_json::from_slice(&out.stdout).unwrap();
    let text = rows.to_string();
    for dep in ["e1", "e2", "e3"] {
        assert!(
            text.contains(&format!("\"{dep}\"")),
            "edge {dep} lost: {text}"
        );
    }
}
