use std::process::Command;

fn pipi(args: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_piping"))
        .args(args)
        .output()
        .expect("run CLI")
}

#[test]
fn help_and_version_require_no_sockets() {
    let help = pipi(&["--help"]);
    assert!(help.status.success());
    let text = String::from_utf8(help.stdout).unwrap();
    assert!(text.contains("pipi fast"));
    assert!(text.contains("pipi range"));
    assert!(text.contains("pipi monitor"));
    assert!(text.contains("pipi discover"));

    let version = pipi(&["--version"]);
    assert!(version.status.success());
    assert!(
        String::from_utf8(version.stdout)
            .unwrap()
            .starts_with("pipi ")
    );
}

#[test]
fn argument_errors_have_exit_code_2() {
    for args in [
        vec!["--bogus"],
        vec!["-4", "-6", "127.0.0.1"],
        vec!["--size", "2", "127.0.0.1"],
        vec!["fast", "127.0.0.1", "--pps", "1001"],
        vec!["range", "192.0.2.0/8"],
        vec!["range", "192.0.2.3-192.0.2.1"],
    ] {
        let result = pipi(&args);
        assert_eq!(result.status.code(), Some(2), "{args:?}");
        assert!(!result.stderr.is_empty(), "{args:?}");
    }
}

#[test]
fn no_implicit_fullscreen_ui() {
    let result = pipi(&["tui", "--help"]);
    assert!(result.status.success());
    assert!(
        !String::from_utf8(result.stdout)
            .unwrap()
            .contains("\x1b[?1049h")
    );
}

#[test]
fn invalid_format_rejected_without_socket() {
    let result = pipi(&["fast", "--output", "xml", "127.0.0.1"]);
    assert_eq!(result.status.code(), Some(2));
}

#[test]
fn discover_help_has_no_socket_or_privilege_requirements() {
    let result = pipi(&["discover", "--help"]);
    assert!(result.status.success());
    let stdout = String::from_utf8(result.stdout).unwrap();
    assert!(stdout.contains("rtnetlink") || stdout.contains("RTM_GETNEIGH"));
    assert!(stdout.contains("--active"));
}

#[test]
fn discovery_invalid_flags_fail_without_network_packets() {
    for args in [
        vec!["discover", "--active", "10.0.0.0/24"],
        vec!["discover", "--active", "10.0.0.1", "-i", "eth0"],
        vec!["discover", "--active", "10.0.0.0/24", "-i", "eth0", "--pps", "101"],
        vec!["discover", "--interface", "../evil"],
        vec!["discover", "--max-results", "4097"],
    ] {
        let result = pipi(&args);
        assert_eq!(result.status.code(), Some(2), "{args:?}");
    }
}
