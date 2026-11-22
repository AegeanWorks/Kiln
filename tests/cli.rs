//! Binary integration tests: the shell's edges are exercised through the built
//! executable, including exit codes and stdout/stderr contracts.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use kiln_core::model::Verdict;

const GOLDEN_JSON: &str = include_str!("../examples/users.json");

fn kiln() -> Command {
    Command::new(env!("CARGO_BIN_EXE_kiln"))
}

fn temp_path(label: &str, extension: &str) -> PathBuf {
    std::env::temp_dir().join(format!(
        "kiln-test-{}-{}.{}",
        std::process::id(),
        label,
        extension
    ))
}

fn write_temp(label: &str, extension: &str, text: &str) -> PathBuf {
    let path = temp_path(label, extension);
    fs::write(&path, text).expect("temp fixture must be writable");
    path
}

fn cleanup(path: &Path) {
    let _ = fs::remove_file(path);
}

fn stdout(output: &Output) -> String {
    String::from_utf8_lossy(&output.stdout).to_string()
}

fn stderr(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).to_string()
}

#[test]
fn inventory_is_machine_readable() {
    let output = kiln().arg("inventory").output().expect("inventory must run");
    assert!(output.status.success());
    for line in stdout(&output).lines().filter(|line| !line.trim().is_empty()) {
        let parts: Vec<&str> = line.split('|').collect();
        assert_eq!(parts.len(), 3, "bad inventory line: {}", line);
        assert!(parts.iter().all(|part| !part.trim().is_empty()));
    }
}

#[test]
fn demo_is_reproducible_for_the_same_seed() {
    let first = kiln().args(["demo", "--scripted", "--seed", "7"]).output().unwrap();
    let second = kiln().args(["demo", "--scripted", "--seed", "7"]).output().unwrap();
    assert!(first.status.success());
    assert_eq!(stdout(&first), stdout(&second));
    assert!(stdout(&first).contains("WORLD-DELTA: records=0->2"));
    assert!(stdout(&first).contains("SEED: 7"));
}

#[test]
fn generate_matches_the_golden_fixture() {
    let output = kiln().args(["generate", "examples/users.kiln"]).output().unwrap();
    assert!(output.status.success(), "{}", stderr(&output));
    assert_eq!(stdout(&output).trim_end(), GOLDEN_JSON.trim_end());
}

#[test]
fn check_gate_passes_for_a_matching_fixture() {
    let output = kiln()
        .args(["check", "examples/users.kiln", "--fixture", "examples/users.json"])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(0));
    assert!(stdout(&output).contains(Verdict::Pass.label()));
}

#[test]
fn check_gate_fails_for_a_stale_fixture() {
    let fixture = write_temp("stale", "json", "{\"stale\": true}\n");
    let output = kiln()
        .args(["check", "examples/users.kiln", "--fixture"])
        .arg(&fixture)
        .output()
        .unwrap();
    cleanup(&fixture);
    assert_eq!(output.status.code(), Some(1));
    assert!(stdout(&output).contains(Verdict::Fail.label()));
}

#[test]
fn invalid_schema_exits_two_with_a_diagnosis() {
    let schema_path = write_temp("bad", "kiln", "version = 9\n");
    let output = kiln().arg("generate").arg(&schema_path).output().unwrap();
    cleanup(&schema_path);
    assert_eq!(output.status.code(), Some(2));
    assert!(stderr(&output).contains("Why:"));
    assert!(stderr(&output).contains("Fix:"));
}

#[test]
fn out_writes_a_file_and_keeps_stdout_clean() {
    let out = temp_path("out", "json");
    let output = kiln()
        .args(["generate", "examples/users.kiln", "--out"])
        .arg(&out)
        .output()
        .unwrap();
    assert!(output.status.success(), "{}", stderr(&output));
    let written = fs::read_to_string(&out).expect("output file must exist");
    cleanup(&out);
    assert_eq!(written.trim_end(), GOLDEN_JSON.trim_end());
    assert_eq!(stdout(&output), "wrote fixture to output file\n");
}

#[test]
fn working_directory_does_not_change_the_bytes() {
    let output = Command::new(env!("CARGO_BIN_EXE_kiln"))
        .current_dir("examples")
        .args(["generate", "users.kiln"])
        .output()
        .unwrap();
    assert!(output.status.success(), "{}", stderr(&output));
    assert_eq!(stdout(&output).trim_end(), GOLDEN_JSON.trim_end());
}
