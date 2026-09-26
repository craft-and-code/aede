//! The server's operating boundaries must be discoverable without its source.

use std::process::Command;

fn help(args: &[&str]) -> String {
    let output = Command::new(env!("CARGO_BIN_EXE_aede"))
        .args(args)
        .env("NO_COLOR", "1")
        .output()
        .expect("run command help");
    assert!(output.status.success(), "{:?}", output.stderr);
    String::from_utf8(output.stdout).expect("UTF-8 help")
}

#[test]
fn server_help_explains_setup_administration_and_the_local_access_boundary() {
    let page = help(&["help", "serve"]);
    assert_eq!(page, help(&["serve", "--help"]));
    for required in [
        "127.0.0.1:8787",
        "0..=65535",
        "--port 0",
        "aede scan <folder>",
        "AEDE_HOME",
        "same data directory",
        "AEDE_ADMIN_TOKEN",
        "32 ASCII",
        "POST /api/admin/v1/scan",
        "POST /api/admin/v1/fetch",
        "GET /api/admin/v1/tasks/<id>",
        "POST /api/admin/v1/tasks/<id>/cancel",
        "PUT /api/admin/v1/annotation?ref=<token>",
        "POST/GET\n  /api/admin/v1/history",
        "GET/PUT/DELETE /api/admin/v1/collection?name=<name>",
        "user.json",
        "persistent playlists",
        "GET /api/v1/albums",
        "/api/v1/doctor",
        "/api/v1/from?name=<artist>",
        "crates/aede-server/README.md",
        "Authorization: Bearer",
        "Ctrl-C",
        "SIGTERM",
        "Windows",
        "audio playback",
        "accounts",
        "remote access",
        "docs/operating.md",
        "docs/api.md",
    ] {
        assert!(page.contains(required), "missing {required:?}:\n{page}");
    }
}

#[test]
fn cancellation_help_explains_which_tasks_can_stop_and_how_to_find_them() {
    let page = help(&["help", "cancel"]);
    assert_eq!(page, help(&["cancel", "--help"]));
    for required in [
        "scan or fetch",
        "print a task ID",
        "same data directory",
        "--data",
        "restarts",
        "Ctrl-C",
        "does not cancel",
        "130",
        "HTTP",
        "already saved",
        "Unix",
        "Windows",
        "no server",
    ] {
        assert!(page.contains(required), "missing {required:?}:\n{page}");
    }
}

#[test]
fn long_command_help_makes_delegation_and_explicit_cancellation_discoverable() {
    for command in ["scan", "fetch"] {
        let page = help(&["help", command]);
        for required in [
            "Unix",
            "same data directory",
            "Ctrl-C",
            "does not cancel",
            "aede cancel <task-id>",
            "no server",
        ] {
            assert!(page.contains(required), "missing {required:?}:\n{page}");
        }
    }
}

#[test]
fn help_exposes_the_server_guide_and_complete_data_directory_precedence() {
    let page = help(&["help"]);
    for required in [
        "aede help serve",
        "AEDE_HOME",
        "XDG_DATA_HOME",
        "~/.local/share/aede",
    ] {
        assert!(page.contains(required), "missing {required:?}:\n{page}");
    }
    let fetch = help(&["help", "fetch"]);
    assert!(fetch.contains("--data <folder>"), "{fetch}");
}
