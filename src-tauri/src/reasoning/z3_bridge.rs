//! Rust client for the Z3 Python sidecar.
//!
//! Spawns `python3 <sidecar>` and communicates line-delimited JSON over
//! stdin / stdout. The first reply from the sidecar is `{"ready": true}` —
//! once we've consumed that the bridge is ready to accept SMT-LIB.
//!
//! Architectural note: keeping Z3 in a Python sidecar (rather than a Rust
//! `z3-sys` binding) is deliberate. The Rust `z3-sys` crate requires a
//! libz3 system library; on user machines that means either a brew/apt
//! install or shipping prebuilt binaries per platform. Python `pip install
//! z3-solver` ships everything in a wheel and is more portable. Sidecar
//! lifetime is tied to `Z3Bridge`'s — drop kills the process.

use std::io::{BufRead, BufReader, Write};
use std::path::PathBuf;
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};
use std::sync::Mutex;

use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum Z3Error {
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    #[error("python3 / sidecar.py not on PATH (run `pip install z3-solver`)")]
    SidecarMissing,
    #[error("sidecar handshake failed: {0}")]
    HandshakeFailed(String),
    #[error("sidecar reported: {0}")]
    SidecarError(String),
    #[error("json: {0}")]
    Json(#[from] serde_json::Error),
}

#[derive(Debug, Clone, Serialize)]
struct Request<'a> {
    id: u64,
    smt: &'a str,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Z3Reply {
    pub id: Option<u64>,
    /// `"sat"`, `"unsat"`, or `"unknown"`.
    pub result: Option<String>,
    /// String form of the Z3 model, e.g. `"[x = 9]"`. `None` for unsat.
    pub model: Option<String>,
    pub error: Option<String>,
}

/// A long-lived handle that owns the sidecar process. Each `solve()` call
/// is serialised through an internal `Mutex` so concurrent callers don't
/// interleave requests on the same stdin.
pub struct Z3Bridge {
    inner: Mutex<Inner>,
}

struct Inner {
    child: Child,
    stdin: ChildStdin,
    stdout: BufReader<ChildStdout>,
    next_id: u64,
}

impl Z3Bridge {
    /// Spawn the sidecar at `sidecar_path` (typically the repo's
    /// `sidecars/z3_sidecar.py`). Returns once the script has reported
    /// `{"ready": true}` — i.e. `import z3` succeeded.
    pub fn spawn(sidecar_path: PathBuf) -> Result<Self, Z3Error> {
        let mut child = Command::new("python3")
            .arg(sidecar_path)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| match e.kind() {
                std::io::ErrorKind::NotFound => Z3Error::SidecarMissing,
                _ => Z3Error::Io(e),
            })?;
        let stdin = child
            .stdin
            .take()
            .ok_or_else(|| Z3Error::HandshakeFailed("no stdin".into()))?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| Z3Error::HandshakeFailed("no stdout".into()))?;
        let mut reader = BufReader::new(stdout);

        let mut line = String::new();
        reader.read_line(&mut line)?;
        let parsed: serde_json::Value = serde_json::from_str(line.trim())
            .map_err(|e| Z3Error::HandshakeFailed(format!("first reply not JSON: {e}: {line}")))?;
        if parsed.get("ready").and_then(|v| v.as_bool()) != Some(true) {
            return Err(Z3Error::HandshakeFailed(format!("expected ready, got {line}")));
        }

        Ok(Self {
            inner: Mutex::new(Inner {
                child,
                stdin,
                stdout: reader,
                next_id: 1,
            }),
        })
    }

    /// Submit an SMT-LIB source to the sidecar and read its reply.
    pub fn solve(&self, smt: &str) -> Result<Z3Reply, Z3Error> {
        let mut guard = self
            .inner
            .lock()
            .map_err(|e| Z3Error::HandshakeFailed(format!("mutex poisoned: {e}")))?;
        let id = guard.next_id;
        guard.next_id += 1;

        let req = Request { id, smt };
        let payload = serde_json::to_string(&req)?;
        guard.stdin.write_all(payload.as_bytes())?;
        guard.stdin.write_all(b"\n")?;
        guard.stdin.flush()?;

        let mut line = String::new();
        guard.stdout.read_line(&mut line)?;
        let reply: Z3Reply = serde_json::from_str(line.trim())?;
        if let Some(err) = &reply.error {
            return Err(Z3Error::SidecarError(err.clone()));
        }
        Ok(reply)
    }
}

impl Drop for Z3Bridge {
    fn drop(&mut self) {
        // The child process is owned via the mutex; if we can't acquire
        // the lock (poisoned because of a panic), there's no clean way to
        // signal the sidecar — fall back to nothing and let the OS reap
        // when the process exits. Best-effort kill via the inner.
        if let Ok(mut g) = self.inner.lock() {
            let _ = g.stdin.flush();
            // Killing is unconditional: the sidecar would also exit on
            // EOF, but kill() is the predictable shutdown signal and
            // avoids relying on Python's stdin-EOF behaviour.
            let _ = g.child.kill();
            let _ = g.child.wait();
        }
    }
}

/// Convenience: spawn against the canonical repo sidecar path.
pub fn default_sidecar_path() -> PathBuf {
    // The Rust crate root is `<repo>/src-tauri/`; the sidecar lives next
    // to it as `<repo>/sidecars/z3_sidecar.py`. Resolve relative to the
    // crate's manifest dir so this works in both `cargo test` and
    // `cargo run` from any cwd.
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .map(|p| p.join("sidecars").join("z3_sidecar.py"))
        .unwrap_or_else(|| PathBuf::from("sidecars/z3_sidecar.py"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sidecar_available() -> bool {
        let path = default_sidecar_path();
        if !path.is_file() {
            return false;
        }
        // python3 + z3 importable?
        let probe = Command::new("python3")
            .args(["-c", "import z3"])
            .output();
        matches!(probe, Ok(o) if o.status.success())
    }

    #[test]
    fn classic_int_constraint_is_sat() {
        if !sidecar_available() {
            eprintln!("skip: python3 / z3-solver not available");
            return;
        }
        let bridge = Z3Bridge::spawn(default_sidecar_path()).expect("spawn");
        let smt = "(declare-const x Int) \
                   (assert (and (> x 0) (< x 10) (> (* x x) 50))) \
                   (check-sat) (get-model)";
        let reply = bridge.solve(smt).expect("solve");
        assert_eq!(reply.result.as_deref(), Some("sat"));
        let model = reply.model.expect("model");
        // x in {8, 9} satisfies > 0, < 10, x² > 50.
        assert!(
            model.contains("x = 8") || model.contains("x = 9"),
            "unexpected model: {model}"
        );
    }

    #[test]
    fn unsatisfiable_constraint_is_unsat() {
        if !sidecar_available() {
            eprintln!("skip: python3 / z3-solver not available");
            return;
        }
        let bridge = Z3Bridge::spawn(default_sidecar_path()).expect("spawn");
        let smt = "(declare-const x Int) \
                   (assert (and (> x 5) (< x 3))) \
                   (check-sat)";
        let reply = bridge.solve(smt).expect("solve");
        assert_eq!(reply.result.as_deref(), Some("unsat"));
        assert!(reply.model.is_none(), "unsat should have no model");
    }

    #[test]
    fn two_sequential_queries_share_the_sidecar() {
        if !sidecar_available() {
            eprintln!("skip: python3 / z3-solver not available");
            return;
        }
        let bridge = Z3Bridge::spawn(default_sidecar_path()).expect("spawn");
        let r1 = bridge
            .solve("(declare-const a Bool) (assert a) (check-sat)")
            .expect("a");
        let r2 = bridge
            .solve("(declare-const b Bool) (assert (not b)) (check-sat)")
            .expect("b");
        assert_eq!(r1.result.as_deref(), Some("sat"));
        assert_eq!(r2.result.as_deref(), Some("sat"));
        // IDs are auto-incremented, so the second reply's id > first.
        assert!(r1.id.unwrap() < r2.id.unwrap());
    }

    #[test]
    fn sidecar_error_is_returned_as_err() {
        if !sidecar_available() {
            eprintln!("skip: python3 / z3-solver not available");
            return;
        }
        let bridge = Z3Bridge::spawn(default_sidecar_path()).expect("spawn");
        // Malformed SMT-LIB — unbalanced parens.
        let r = bridge.solve("(declare-const x Int");
        assert!(r.is_err(), "expected SidecarError for malformed SMT");
    }
}
