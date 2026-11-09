use std::process::Command;

#[test]
fn without_arguments_prints_help_and_succeeds() {
    let output = Command::new(env!("CARGO_BIN_EXE_xmtr"))
        .output()
        .expect("xmtr starts");
    assert!(output.status.success(), "exit status {}", output.status);
    let stdout = String::from_utf8(output.stdout).expect("help is UTF-8");
    assert!(stdout.contains("Usage: xmtr"), "help text:\n{stdout}");
}

#[test]
fn version_prints_the_package_version() {
    let output = Command::new(env!("CARGO_BIN_EXE_xmtr"))
        .arg("--version")
        .output()
        .expect("xmtr starts");
    assert!(output.status.success(), "exit status {}", output.status);
    assert_eq!(
        String::from_utf8(output.stdout).expect("version is UTF-8"),
        format!("xmtr {}\n", env!("CARGO_PKG_VERSION"))
    );
}

/// Options of a trace before a subcommand stop xmtr before it resolves or traces anything. The
/// environment keeps a run that did start away from the user's files and the default gateway.
#[test]
fn a_subcommand_after_trace_options_fails() {
    let home = std::path::Path::new(env!("CARGO_TARGET_TMPDIR")).join("subcommand-after-options");
    for (args, hint) in [
        (
            &["--dns", "quad9", "traces", "delete", "1-a"][..],
            "traces is a subcommand and must come before any option; put its options after it, such as xmtr traces delete --dns quad9 <TRACE>",
        ),
        (
            &["--no-rdns", "self", "update"],
            "self is a subcommand and must come before any option; put its options after it, such as xmtr self update --force",
        ),
    ] {
        let output = Command::new(env!("CARGO_BIN_EXE_xmtr"))
            .args(args)
            .env("HOME", &home)
            .env("XDG_DATA_HOME", &home)
            .env("APPDATA", &home)
            .env("DO_NOT_TRACK", "1")
            .env("XMTR_API", "http://127.0.0.1:9")
            .output()
            .expect("xmtr starts");
        let stderr = String::from_utf8(output.stderr).expect("errors are UTF-8");
        assert_eq!(output.status.code(), Some(2), "{args:?}: {stderr}");
        assert!(
            stderr.starts_with(&format!(
                "error: {hint}
"
            )),
            "{args:?}: {stderr}"
        );
        assert!(output.stdout.is_empty(), "{args:?}");
    }
}

#[test]
fn options_without_a_target_fail() {
    let output = Command::new(env!("CARGO_BIN_EXE_xmtr"))
        .arg("--no-upload")
        .output()
        .expect("xmtr starts");
    assert_eq!(output.status.code(), Some(2));
}
