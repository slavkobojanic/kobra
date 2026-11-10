//! Child processes that can't outlive the test that started them
//! (`docs/compiler/testing.md` §2.1, "Child processes").
//!
//! A test that starts a server and then fails an assertion unwinds past
//! the code that would have stopped it. A bare [`Child`] does nothing
//! when dropped, so the server kept running, holding its port, its
//! database and its scratch directory, until someone killed it by hand.
//! Every process a test starts is therefore spawned with
//! [`SpawnGuarded::spawn_guarded`]: the [`ChildGuard`] it returns kills
//! the process and every process below it, and reaps it, when dropped,
//! panicking or not.
//!
//! The children stay in the test's process group, so a Ctrl-C at the
//! terminal and cargo-nextest's signals on a timeout (it signals each
//! test's process group) still reach them when the test process itself is
//! killed and no destructor runs. Their own children are found through
//! `pgrep -P`, before the child is killed and they are handed to `init`.

use std::ops::{Deref, DerefMut};
use std::process::{Child, Command, Output, Stdio};

/// A running child process, killed together with its descendants and
/// reaped when dropped. It derefs to the [`Child`], so a test waits for it,
/// takes its pipes and reads its id as before.
pub struct ChildGuard {
    child: Option<Child>,
}

/// [`Command::spawn`], returning a [`ChildGuard`].
pub trait SpawnGuarded {
    /// Starts the command; the process is killed when the guard is dropped.
    fn spawn_guarded(&mut self) -> std::io::Result<ChildGuard>;
}

impl SpawnGuarded for Command {
    fn spawn_guarded(&mut self) -> std::io::Result<ChildGuard> {
        self.spawn().map(ChildGuard::new)
    }
}

impl ChildGuard {
    /// Takes ownership of `child`.
    pub fn new(child: Child) -> ChildGuard {
        ChildGuard { child: Some(child) }
    }

    /// [`Child::wait_with_output`]: waits for the process to exit by itself
    /// and collects its output. Its descendants are killed afterwards.
    pub fn wait_with_output(mut self) -> std::io::Result<Output> {
        let child = self.child.take().ok_or_else(gone)?;
        let tree = descendants(child.id());
        let out = child.wait_with_output();
        kill_all(&tree);
        out
    }
}

fn gone() -> std::io::Error {
    std::io::Error::other("the child was already taken")
}

impl Deref for ChildGuard {
    type Target = Child;
    fn deref(&self) -> &Child {
        self.child
            .as_ref()
            .unwrap_or_else(|| unreachable!("taken only by wait_with_output"))
    }
}

impl DerefMut for ChildGuard {
    fn deref_mut(&mut self) -> &mut Child {
        self.child
            .as_mut()
            .unwrap_or_else(|| unreachable!("taken only by wait_with_output"))
    }
}

impl ChildGuard {
    /// Kills the process and its descendants now and reaps it, as dropping
    /// the guard does; for an owner that must clean up after it (Chrome's
    /// profile) in its own `Drop`. Calling it again does nothing.
    pub fn kill_tree(&mut self) {
        let Some(child) = self.child.as_mut() else {
            return;
        };
        // Once the child has exited (and `try_wait` reaped it) its children
        // belong to `init` and can't be found any more; while it runs, they
        // are listed first, so killing it doesn't orphan them.
        let tree = if matches!(child.try_wait(), Ok(None)) {
            descendants(child.id())
        } else {
            Vec::new()
        };
        // The child first: a supervisor would replace a worker killed
        // before it.
        let _ = child.kill();
        kill_all(&tree);
        let _ = child.wait();
    }
}

impl Drop for ChildGuard {
    fn drop(&mut self) {
        self.kill_tree();
    }
}

/// Every live descendant of `pid`, children before grandchildren.
fn descendants(pid: u32) -> Vec<u32> {
    let mut out = Vec::new();
    if !cfg!(unix) {
        return out;
    }
    let mut todo = vec![pid];
    while let Some(parent) = todo.pop() {
        let Ok(listed) = Command::new("pgrep")
            .args(["-P", &parent.to_string()])
            .stdin(Stdio::null())
            .stderr(Stdio::null())
            .output()
        else {
            // No `pgrep`: the child alone is killed.
            break;
        };
        for p in String::from_utf8_lossy(&listed.stdout)
            .split_whitespace()
            .filter_map(|p| p.parse::<u32>().ok())
        {
            // Never 0 or 1: to `kill`, they mean a process group and
            // every process of the user.
            if p > 1 && p != pid && !out.contains(&p) {
                out.push(p);
                todo.push(p);
            }
        }
    }
    out
}

/// `SIGKILL` to each of `pids`.
fn kill_all(pids: &[u32]) {
    if pids.is_empty() || !cfg!(unix) {
        return;
    }
    let _ = Command::new("kill")
        .arg("-KILL")
        .args(pids.iter().map(u32::to_string))
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .status();
}

/// Whether `pid` is a live process (a zombie counts as gone: it runs
/// nothing and holds no port).
#[cfg(unix)]
pub fn alive(pid: u32) -> bool {
    Command::new("ps")
        .args(["-o", "stat=", "-p", &pid.to_string()])
        .stderr(Stdio::null())
        .output()
        .is_ok_and(|o| {
            let stat = String::from_utf8_lossy(&o.stdout);
            let stat = stat.trim();
            !stat.is_empty() && !stat.starts_with('Z')
        })
}

#[cfg(all(test, unix))]
mod tests {
    use std::io::{BufRead, BufReader};
    use std::panic::AssertUnwindSafe;
    use std::process::{Command, Stdio};
    use std::time::{Duration, Instant};

    use super::{SpawnGuarded, alive};

    /// Waits up to 10 s for `pid` to be gone.
    fn gone_soon(pid: u32) -> bool {
        let deadline = Instant::now() + Duration::from_secs(10);
        while alive(pid) {
            if Instant::now() >= deadline {
                return false;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        true
    }

    /// A shell that starts a `sleep` of its own and waits for it: the
    /// guard, its child (the shell's id) and its grandchild (the sleep's).
    fn shell_with_a_child() -> (super::ChildGuard, u32, u32) {
        let mut shell = Command::new("sh")
            .args(["-c", "sleep 300 & echo $!; wait"])
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn_guarded()
            .expect("sh starts");
        let mut line = String::new();
        BufReader::new(shell.stdout.take().expect("piped"))
            .read_line(&mut line)
            .expect("the sleep's id");
        let sleep = line.trim().parse().expect("a process id");
        let id = shell.id();
        (shell, id, sleep)
    }

    #[test]
    fn a_panic_with_a_guarded_child_kills_it_and_its_children() {
        let mut ids = (0, 0);
        let unwound = std::panic::catch_unwind(AssertUnwindSafe(|| {
            let (_shell, shell, sleep) = shell_with_a_child();
            ids = (shell, sleep);
            assert!(alive(shell) && alive(sleep), "both run");
            panic!("a failing test with its server still running");
        }));
        assert!(unwound.is_err(), "the block panicked");
        let (shell, sleep) = ids;
        assert!(shell > 1 && sleep > 1, "{ids:?}");
        assert!(gone_soon(shell), "the child {shell} was killed and reaped");
        assert!(gone_soon(sleep), "its child {sleep} was killed too");
    }

    #[test]
    fn a_dropped_guard_leaves_nothing_running() {
        let (guard, shell, sleep) = shell_with_a_child();
        drop(guard);
        assert!(gone_soon(shell), "the child {shell} was killed and reaped");
        assert!(gone_soon(sleep), "its child {sleep} was killed too");
    }

    #[test]
    fn a_child_that_already_exited_is_reaped_quietly() {
        let mut guard = Command::new("true")
            .stdin(Stdio::null())
            .spawn_guarded()
            .expect("true starts");
        let id = guard.id();
        assert!(guard.wait().expect("exits").success());
        drop(guard);
        assert!(gone_soon(id));
    }
}
