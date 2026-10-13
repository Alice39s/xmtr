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

#[test]
fn options_without_a_target_fail() {
    let output = Command::new(env!("CARGO_BIN_EXE_xmtr"))
        .arg("--no-upload")
        .output()
        .expect("xmtr starts");
    assert_eq!(output.status.code(), Some(2));
}
