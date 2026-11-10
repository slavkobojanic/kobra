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
//!
//! A reservation only keeps other test processes away. [`in_use`] checks
//! that nothing else holds the port either, on any local address: on macOS
//! binding `127.0.0.1:<port>` succeeds next to a server listening on
//! `*:<port>` (Rust's listeners set `SO_REUSEADDR`, and a specific address
//! is a different socket to the wildcard one), so a port a stale server
//! still listens on looked free, the app started there failed to bind
//! `0.0.0.0`, and the test talked to the stale server instead.

use std::fs::{File, OpenOptions};
use std::io::ErrorKind;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr, TcpListener, TcpStream};
use std::sync::Mutex;
use std::time::Duration;

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

/// Whether anything holds TCP `port` on this machine: something answers
/// on `127.0.0.1` or `[::1]`, or `127.0.0.1`, `0.0.0.0` or `[::]` can't be
/// bound (a machine without IPv6, where `[::]` can't be bound at all,
/// doesn't count as holding it).
pub fn in_use(port: u16) -> bool {
    let answers = |ip: IpAddr| {
        TcpStream::connect_timeout(&SocketAddr::new(ip, port), Duration::from_millis(200)).is_ok()
    };
    let binds = |ip: IpAddr| TcpListener::bind(SocketAddr::new(ip, port)).map(drop);
    answers(Ipv4Addr::LOCALHOST.into())
        || answers(Ipv6Addr::LOCALHOST.into())
        || binds(Ipv4Addr::LOCALHOST.into()).is_err()
        || binds(Ipv4Addr::UNSPECIFIED.into()).is_err()
        || binds(Ipv6Addr::UNSPECIFIED.into()).is_err_and(|e| e.kind() == ErrorKind::AddrInUse)
}

/// Reserves `port` ([`reserve`]) when nothing holds it ([`in_use`]).
pub fn take(port: u16) -> bool {
    reserve(port) && !in_use(port)
}

#[cfg(test)]
mod tests {
    use std::net::TcpListener;

    use super::{in_use, take};

    /// A port in 5200–5999 that this process has reserved and nothing holds.
    fn a_free_port() -> u16 {
        (5200..6000u16)
            .find(|&p| take(p))
            .expect("a free port in 5200–5999")
    }

    #[test]
    fn a_port_with_a_listener_on_the_wildcard_address_is_in_use() {
        let port = a_free_port();
        assert!(!in_use(port), "{port} is free before");
        let wildcard = TcpListener::bind(("0.0.0.0", port)).expect("binds *:port");
        // What fooled the old check, on macOS.
        #[cfg(target_os = "macos")]
        assert!(
            TcpListener::bind(("127.0.0.1", port)).is_ok(),
            "a loopback bind still succeeds next to *:{port}"
        );
        assert!(in_use(port), "*:{port} has a listener");
        drop(wildcard);
        assert!(!in_use(port), "{port} is free again");
    }

    #[test]
    fn a_port_with_a_listener_on_loopback_or_ipv6_is_in_use() {
        let port = a_free_port();
        let loopback = TcpListener::bind(("127.0.0.1", port)).expect("binds");
        assert!(in_use(port), "127.0.0.1:{port} has a listener");
        drop(loopback);
        if let Ok(v6) = TcpListener::bind(("::", port)) {
            assert!(in_use(port), "[::]:{port} has a listener");
            drop(v6);
        }
        assert!(!in_use(port));
    }
}
