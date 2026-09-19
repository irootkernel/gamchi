//! Process-group teardown without live Grok.

use samchi_adapter_grok::teardown_process_group;
use std::io::{BufRead, BufReader};
use std::os::unix::process::CommandExt as _;
use std::process::{Command, Stdio};
use std::thread;
use std::time::Duration;

#[test]
fn teardown_kills_process_group_not_only_leader() {
    let mut leader = Command::new("sh");
    leader
        .args(["-c", "sleep 30 & echo $!; wait"])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    leader.process_group(0);
    let mut child = leader.spawn().expect("spawn group");
    let pid = child.id();
    let stdout = child.stdout.take().expect("stdout");
    let mut line = String::new();
    BufReader::new(stdout)
        .read_line(&mut line)
        .expect("grandchild pid");
    let grandchild: u32 = line.trim().parse().expect("pid");
    assert_ne!(grandchild, 0);
    assert_ne!(grandchild, pid, "grandchild should differ from leader");
    let g_before = unsafe { libc::kill(grandchild as i32, 0) };
    assert_eq!(g_before, 0, "grandchild {grandchild} should be running");
    teardown_process_group(pid);
    thread::sleep(Duration::from_millis(200));
    match child.try_wait() {
        Ok(Some(_)) => {}
        Ok(None) => panic!("group leader {pid} still running after teardown"),
        Err(err) if err.raw_os_error() == Some(libc::ECHILD) => {}
        Err(err) => panic!("wait: {err}"),
    }
    let alive = unsafe { libc::kill(pid as i32, 0) };
    assert_ne!(alive, 0, "leader pid {pid} still accepts signals");
    let g_after = unsafe { libc::kill(grandchild as i32, 0) };
    assert_ne!(g_after, 0, "grandchild {grandchild} still accepts signals");
}

#[test]
fn teardown_returns_quickly_when_leader_is_zombie() {
    let mut leader = Command::new("sh");
    leader
        .args(["-c", "exit 0"])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    leader.process_group(0);
    let mut child = leader.spawn().expect("spawn zombie");
    let pid = child.id();
    thread::sleep(Duration::from_millis(50));
    let start = std::time::Instant::now();
    teardown_process_group(pid);
    let elapsed = start.elapsed();
    assert!(
        elapsed < Duration::from_millis(400),
        "zombie wait took {elapsed:?}"
    );
    let _ = child.try_wait();
}
