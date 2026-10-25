//! Integration tests for config check CLI behavior.

use std::{fs, process::Command};

#[test]
fn config_check_succeeds_for_valid_fixture() -> Result<(), Box<dyn std::error::Error>> {
    let dir = tempfile::tempdir()?;
    let path = dir.path().join("config.toml");
    fs::write(
        &path,
        r#"
        [download]
        audio_quality = "high"
        skip_existing = true

        [listener]
        port = 8123
        secret = ""
        "#,
    )?;

    let output = Command::new(env!("CARGO_BIN_EXE_riptide-rs"))
        .args(["config", "check", "--path"])
        .arg(&path)
        .output()?;

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout)?;
    assert!(stdout.contains("config ok"));
    Ok(())
}

#[test]
fn config_check_fails_for_invalid_fixture_with_context() -> Result<(), Box<dyn std::error::Error>> {
    let dir = tempfile::tempdir()?;
    let path = dir.path().join("config.toml");
    fs::write(
        &path,
        r#"
        [download]
        audio_quality = "normal"
        "#,
    )?;

    let output = Command::new(env!("CARGO_BIN_EXE_riptide-rs"))
        .args(["config", "check", "--path"])
        .arg(&path)
        .output()?;

    assert!(!output.status.success());
    let stderr = String::from_utf8(output.stderr)?;
    assert!(stderr.contains("failed to validate config"));
    assert!(stderr.contains("invalid config value at `download.audio_quality`"));
    Ok(())
}
