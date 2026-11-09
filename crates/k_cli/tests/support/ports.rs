//! Port reservations shared by every test process on the machine.
//!
//! Probing a port by binding it and closing the listener again leaves a
//! window in which another test process probes the same port and both
//! start a server on it. Under cargo-nextest every test is a process of
//! its own, so an in-process counter can't prevent that. [`reserve`]
//! takes an advisory lock on a file named after the port in a directory
//! of the system's temp dir and keeps it until the process exits (the
//! operating system releases it then, even after a crash), so a port is
//! handed to at most one live test process at a time.

use std::fs::{File, OpenOptions};
use std::sync::Mutex;

/// The locks this process holds; kept open until it exits.
static HELD: Mutex<Vec<File>> = Mutex::new(Vec::new());

/// Reserves `port` for this process. `false` when another live test
/// process holds it. When the lock file can't be opened at all (a
/// read-only temp dir) the port counts as reserved, so callers fall back
/// to probing alone.
// `File::try_lock` is stable since 1.89; the workspace needs 1.97
// (Cargo.toml), clippy.toml's older MSRV notwithstanding.
#[allow(clippy::incompatible_msrv)]
pub fn reserve(port: u16) -> bool {
    let dir = std::env::temp_dir().join("k-test-ports");
    let _ = std::fs::create_dir_all(&dir);
    let Ok(file) = OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(false)
        .open(dir.join(format!("{port}.lock")))
    else {
        return true;
    };
    match file.try_lock() {
        Ok(()) => {
            if let Ok(mut held) = HELD.lock() {
                held.push(file);
            }
            true
        }
        Err(_) => false,
    }
}
