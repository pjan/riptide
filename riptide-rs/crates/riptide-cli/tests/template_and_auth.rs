//! Integration tests for template CLI and auth status edges.

use std::{fs, process::Command};

#[test]
fn template_check_succeeds_for_python_style_template() -> Result<(), Box<dyn std::error::Error>> {
    let output = Command::new(env!("CARGO_BIN_EXE_riptide-rs"))
        .args([
            "template",
            "check",
            "{album.artist}/{album.title}/{item.number:02d} - {item.explicit:long,upper} - {item.title_version}",
        ])
        .output()?;

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout)?;
    assert!(stdout.contains("template ok"));
    Ok(())
}

#[test]
fn template_render_fixture_is_stable() -> Result<(), Box<dyn std::error::Error>> {
    let dir = tempfile::tempdir()?;
    let fixture = dir.path().join("fixture.json");
    fs::write(
        &fixture,
        r#"
        {
          "now": "2024-01-15T12:00:00Z",
          "item": {
            "kind": "track",
            "id": "1",
            "title": "Track/Title",
            "number": 7,
            "volume": 1,
            "version": "Radio Edit",
            "bpm": 0,
            "quality": "LOSSLESS",
            "artist": "Main Artist",
            "artists": "Main Artist",
            "features": "Featured Artist",
            "artists_with_features": "Main Artist, Featured Artist",
            "explicit": true,
            "dolby": false
          },
          "album": {
            "id": "2",
            "title": "Album",
            "artist": "Main Artist",
            "artists": "Main Artist",
            "date": "2024-01-01",
            "explicit": false,
            "master": false,
            "release": "ALBUM"
          },
          "extras": {
            "type": "track"
          }
        }
        "#,
    )?;

    let output = Command::new(env!("CARGO_BIN_EXE_riptide-rs"))
        .args([
            "template",
            "render-fixture",
            "{type}/{album.artist}/{album.title}/{item.number:02d} - {item.title_version}",
            "--fixture",
        ])
        .arg(&fixture)
        .arg("--no-asterisk-ext")
        .output()?;

    assert!(
        output.status.success(),
        "stderr={}",
        String::from_utf8(output.stderr)?
    );
    let stdout = String::from_utf8(output.stdout)?;
    assert_eq!(
        stdout.trim(),
        "track/Main Artist/Album/07 - TrackTitle (Radio Edit)"
    );
    Ok(())
}

#[test]
fn auth_status_does_not_require_credentials() -> Result<(), Box<dyn std::error::Error>> {
    let dir = tempfile::tempdir()?;
    let auth_file = dir.path().join("missing-token.json");
    let output = Command::new(env!("CARGO_BIN_EXE_riptide-rs"))
        .args(["--auth-file"])
        .arg(auth_file)
        .args(["auth", "status"])
        .output()?;

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout)?;
    assert!(stdout.contains("not authenticated"));
    Ok(())
}
