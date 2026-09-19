//! Process-group teardown for a parent-owned `grok agent stdio` child.

use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::time::{Duration, Instant};

const MAX_LIVE: usize = 64;

static SHUTTING_DOWN: AtomicBool = AtomicBool::new(false);
static SPAWNS_OPEN: AtomicU32 = AtomicU32::new(0);
static LIVE_PIDS: [AtomicU32; MAX_LIVE] = [const { AtomicU32::new(0) }; MAX_LIVE];

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
            libc::kill(pgid, libc::SIGTERM);
        }
        let deadline = Instant::now() + Duration::from_secs(2);
        let mut leader_gone = false;
        while Instant::now() < deadline {
            if !pid_alive(pid) {
                leader_gone = true;
                break;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        unsafe {
            libc::killpg(pgid, libc::SIGKILL);
            libc::kill(pgid, libc::SIGKILL);
        }
        if leader_gone {
            return;
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

/// Tracks a spawned ACP child until drop, then tears down its process group.
pub struct ChildReap {
    pid: u32,
}

impl Drop for ChildReap {
    fn drop(&mut self) {
        teardown_process_group(self.pid);
        unregister_child_pid(self.pid);
    }
}

/// Held while an ACP child is being spawned so parent-exit waits for register.
pub(crate) struct AcpSpawnGuard(());

impl Drop for AcpSpawnGuard {
    fn drop(&mut self) {
        SPAWNS_OPEN.fetch_sub(1, Ordering::SeqCst);
    }
}

pub(crate) fn begin_acp_spawn() -> AcpSpawnGuard {
    SPAWNS_OPEN.fetch_add(1, Ordering::SeqCst);
    AcpSpawnGuard(())
}

/// Live pid table has no free slot.
#[derive(Debug)]
pub struct LiveTableFull;

/// Register `pid` for parent-exit teardown. Drop kills the process group.
/// `Err` if the live table is full; the process group is torn down first.
pub fn track_child(pid: u32) -> Result<ChildReap, LiveTableFull> {
    if pid == 0 {
        return Ok(ChildReap { pid: 0 });
    }
    if !register_child_pid(pid) {
        teardown_process_group(pid);
        return Err(LiveTableFull);
    }
    Ok(ChildReap { pid })
}

pub fn shutting_down() -> bool {
    SHUTTING_DOWN.load(Ordering::SeqCst)
}

/// Graceful parent exit: refuse new children, tear down registered groups.
/// Always polls for a short deadline so an admit-before-`track_child` spawn
/// can still register.
pub fn parent_exit_teardown() {
    SHUTTING_DOWN.store(true, Ordering::SeqCst);
    let deadline = Instant::now() + Duration::from_millis(500);
    let hard = Instant::now() + Duration::from_secs(2);
    loop {
        teardown_registered_children();
        let spawning = SPAWNS_OPEN.load(Ordering::SeqCst) != 0;
        let now = Instant::now();
        if now >= hard || (now >= deadline && !spawning) {
            teardown_registered_children();
            break;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
}

/// SIGINT/SIGTERM: kill registered groups and `_exit`. Not for tests of this crate.
#[cfg(unix)]
pub fn install_signal_teardown() {
    unsafe {
        libc::signal(
            libc::SIGINT,
            signal_teardown as *const () as libc::sighandler_t,
        );
        libc::signal(
            libc::SIGTERM,
            signal_teardown as *const () as libc::sighandler_t,
        );
    }
}

#[cfg(not(unix))]
pub fn install_signal_teardown() {}

#[cfg(unix)]
extern "C" fn signal_teardown(sig: libc::c_int) {
    SHUTTING_DOWN.store(true, Ordering::SeqCst);
    for slot in &LIVE_PIDS {
        let pid = slot.load(Ordering::SeqCst);
        if pid != 0 {
            unsafe {
                libc::killpg(pid as i32, libc::SIGKILL);
                libc::kill(pid as i32, libc::SIGKILL);
            }
        }
    }
    unsafe {
        libc::_exit(128 + sig);
    }
}

fn register_child_pid(pid: u32) -> bool {
    if pid == 0 {
        return true;
    }
    for slot in &LIVE_PIDS {
        if slot
            .compare_exchange(0, pid, Ordering::SeqCst, Ordering::SeqCst)
            .is_ok()
        {
            return true;
        }
    }
    false
}

fn unregister_child_pid(pid: u32) {
    if pid == 0 {
        return;
    }
    for slot in &LIVE_PIDS {
        let _ = slot.compare_exchange(pid, 0, Ordering::SeqCst, Ordering::SeqCst);
    }
}

fn teardown_registered_children() {
    for slot in &LIVE_PIDS {
        let pid = slot.swap(0, Ordering::SeqCst);
        if pid != 0 {
            teardown_process_group(pid);
        }
    }
}

#[cfg(unix)]
fn pid_alive(pid: u32) -> bool {
    let mut status = 0;
    let rc = unsafe { libc::waitpid(pid as i32, &mut status, libc::WNOHANG) };
    if rc > 0 {
        return false;
    }
    if rc == 0 {
        return true;
    }
    let k = unsafe { libc::kill(pid as i32, 0) };
    if k == 0 {
        return true;
    }
    std::io::Error::last_os_error().raw_os_error() != Some(libc::ESRCH)
}
