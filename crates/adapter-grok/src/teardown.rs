//! Process-group teardown for a parent-owned `grok agent stdio` child.

use std::time::{Duration, Instant};

/// Kill the child's process group (spawned with `process_group(0)`) and wait
/// briefly so grandchildren die with the in-flight prompt.
pub fn teardown_process_group(pid: u32) {
    if pid == 0 {
        return;
    }
    #[cfg(unix)]
    {
        let pgid = pid as i32;
        unsafe {
            libc::killpg(pgid, libc::SIGTERM);
        }
        let deadline = Instant::now() + Duration::from_secs(2);
        while Instant::now() < deadline {
            if !pid_alive(pid) {
                return;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        unsafe {
            libc::killpg(pgid, libc::SIGKILL);
        }
        let deadline = Instant::now() + Duration::from_millis(500);
        while Instant::now() < deadline {
            if !pid_alive(pid) {
                return;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
    }
    #[cfg(not(unix))]
    {
        let _ = pid;
    }
}

#[cfg(unix)]
fn pid_alive(pid: u32) -> bool {
    let rc = unsafe { libc::kill(pid as i32, 0) };
    if rc == 0 {
        return true;
    }
    std::io::Error::last_os_error().raw_os_error() != Some(libc::ESRCH)
}
