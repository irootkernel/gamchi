//! Process-group teardown without live Grok.

use samchi_adapter_grok::teardown_process_group;
use std::os::unix::process::CommandExt as _;
use std::process::{Command, Stdio};
use std::thread;
use std::time::Duration;

#[test]
fn teardown_kills_process_group_not_only_leader() {
    let mut leader = Command::new("sh");
    leader
        .args(["-c", "sleep 30 & wait"])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    leader.process_group(0);
    let mut child = leader.spawn().expect("spawn group");
    let pid = child.id();
    thread::sleep(Duration::from_millis(100));
    teardown_process_group(pid);
    thread::sleep(Duration::from_millis(200));
    let status = child.try_wait().expect("wait");
    assert!(
        status.is_some(),
        "group leader {pid} still running after teardown"
    );
    let alive = unsafe { libc::kill(pid as i32, 0) };
    assert_ne!(alive, 0, "leader pid {pid} still accepts signals");
}
