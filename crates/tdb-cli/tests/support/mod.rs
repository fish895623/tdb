//! Helpers shared by the end-to-end tests: how to start `tdbd`, and a handle
//! on a running one.

use std::path::Path;
use std::process::Command;

/// `tdbd` command with a clean environment (no inherited RUST_LOG or
/// TDB_CONFIG) so the results don't depend on the developer's shell.
pub fn tdbd_command(dir: &Path, args: &[&str], envs: &[(&str, &str)]) -> Command {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_tdbd"));
    cmd.current_dir(dir)
        .args(args)
        .env_remove("RUST_LOG")
        .env_remove("TDB_CONFIG");
    for (key, value) in envs {
        cmd.env(key, value);
    }
    cmd
}

#[cfg(unix)]
pub use daemon::Daemon;

#[cfg(unix)]
mod daemon {
    use std::io::{BufRead, BufReader};
    use std::net::SocketAddr;
    use std::path::Path;
    use std::process::{Child, Command, ExitStatus, Stdio};
    use std::sync::mpsc::{Receiver, RecvTimeoutError, channel};
    use std::thread;
    use std::time::{Duration, Instant};

    use super::tdbd_command;

    const TIMEOUT: Duration = Duration::from_secs(20);

    /// A running `tdbd`, killed on drop so a failed assertion cannot leave a
    /// stray daemon behind.
    pub struct Daemon {
        pub child: Child,
        lines: Receiver<String>,
        seen: Vec<String>,
    }

    impl Daemon {
        /// Start `tdbd` in `dir` listening on a free port on the loopback
        /// interface, so tests can run side by side and never meet a real
        /// `tdbd` on the default port. [`Daemon::addr`] says which port.
        pub fn spawn(dir: &Path, args: &[&str]) -> Daemon {
            let mut all = vec!["--listen", "127.0.0.1:0"];
            all.extend_from_slice(args);
            Daemon::spawn_raw(dir, &all)
        }

        /// Start `tdbd` in `dir` with exactly `args`.
        pub fn spawn_raw(dir: &Path, args: &[&str]) -> Daemon {
            let mut child = tdbd_command(dir, args, &[])
                .stdout(Stdio::piped())
                .stderr(Stdio::null())
                .spawn()
                .unwrap();
            let out = child.stdout.take().unwrap();
            let (tx, lines) = channel();
            // A thread, so waiting for a line can time out.
            thread::spawn(move || {
                for line in BufReader::new(out).lines().map_while(Result::ok) {
                    if tx.send(line).is_err() {
                        break;
                    }
                }
            });
            Daemon {
                child,
                lines,
                seen: Vec::new(),
            }
        }

        /// Block until a stdout line contains `needle`.
        pub fn wait_for(&mut self, needle: &str) {
            let deadline = Instant::now() + TIMEOUT;
            loop {
                let left = deadline.saturating_duration_since(Instant::now());
                match self.lines.recv_timeout(left) {
                    Ok(line) => {
                        let found = line.contains(needle);
                        self.seen.push(line);
                        if found {
                            return;
                        }
                    }
                    Err(RecvTimeoutError::Timeout) => {
                        panic!("no {needle:?} within {TIMEOUT:?}; saw {:?}", self.seen)
                    }
                    Err(RecvTimeoutError::Disconnected) => {
                        panic!("tdbd exited before {needle:?}; saw {:?}", self.seen)
                    }
                }
            }
        }

        /// Wait until `tdbd` is running and return the address it listens on,
        /// read from its "listening on ADDR;" line.
        pub fn addr(&mut self) -> SocketAddr {
            self.wait_for("listening on ");
            let line = self.seen.last().unwrap();
            let after = line.split_once("listening on ").unwrap().1;
            let addr = after.split_once(';').map_or(after, |(addr, _)| addr);
            addr.trim()
                .parse()
                .unwrap_or_else(|e| panic!("bad address in {line:?}: {e}"))
        }

        pub fn signal(&self, name: &str) {
            let status = Command::new("kill")
                .arg(format!("-{name}"))
                .arg(self.child.id().to_string())
                .status()
                .unwrap();
            assert!(status.success(), "kill -{name} failed");
        }

        pub fn wait_exit(&mut self) -> ExitStatus {
            let deadline = Instant::now() + TIMEOUT;
            while Instant::now() < deadline {
                if let Some(status) = self.child.try_wait().unwrap() {
                    return status;
                }
                thread::sleep(Duration::from_millis(20));
            }
            panic!(
                "tdbd still running {TIMEOUT:?} after the signal; saw {:?}",
                self.seen
            );
        }

        /// Everything printed so far, including after the process has exited.
        pub fn output(&mut self) -> String {
            self.seen.extend(self.lines.try_iter());
            self.seen.join("\n")
        }

        /// Ask it to stop with SIGTERM, check that it exits cleanly, and
        /// return what it printed.
        pub fn stop(mut self) -> String {
            self.signal("TERM");
            let status = self.wait_exit();
            assert!(status.success(), "{status}");
            self.output()
        }
    }

    impl Drop for Daemon {
        fn drop(&mut self) {
            let _ = self.child.kill();
            let _ = self.child.wait();
        }
    }
}
