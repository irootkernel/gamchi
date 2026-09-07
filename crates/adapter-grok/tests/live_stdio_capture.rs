//! Parse the checked-in TASK-004 live `grok agent stdio` capture.
//!
//! Does not spawn Grok. Drives `samchi_adapter_grok::live_capture`.

use samchi_adapter_grok::live_capture::{capture_dir, parse_capture_dir};

#[test]
fn checked_in_live_stdio_capture() {
    let dir = capture_dir(env!("CARGO_MANIFEST_DIR"));
    let parsed = parse_capture_dir(&dir).unwrap_or_else(|err| panic!("{err}"));

    assert!(
        parsed.initialize_request,
        "missing initialize request in {}",
        dir.display()
    );
    assert!(parsed.initialize_result, "missing initialize result");
    assert!(
        !parsed.grok_agent_version.is_empty(),
        "missing grok agent version"
    );
    assert!(
        !parsed.grok_cli_version.is_empty(),
        "missing grok CLI version"
    );
    assert!(
        parsed.grok_cli_version.contains(&parsed.grok_agent_version),
        "CLI version {:?} should mention agent version {:?}",
        parsed.grok_cli_version,
        parsed.grok_agent_version
    );
    assert!(parsed.session_new, "missing session/new");
    assert!(parsed.session_prompt, "missing session/prompt");
    assert!(parsed.session_update, "missing session/update");
    assert!(parsed.edit_kind, "missing tool call kind=edit");
    assert!(
        parsed.fs_write_text_file,
        "missing fs/write_text_file (the real file write)"
    );
    assert_eq!(parsed.stop_reason, "end_turn");
    assert!(
        parsed
            .written_content
            .contains("samchi-for-grok-task-004-live-capture"),
        "written file missing marker, got {:?}",
        parsed.written_content
    );
    assert!(
        parsed.launched_with_no_leader(),
        "argv={:?} owner={:?}",
        parsed.argv,
        parsed.owner
    );
    assert!(
        !parsed.contains_fake_stub_ids,
        "live capture must not contain TASK-003 stub ids"
    );
}
