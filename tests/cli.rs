//! Binary integration tests: the shell's edges are exercised through the built
//! executable, including exit codes and stdout/stderr contracts.

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

use kiln_core::model::Verdict;

const GOLDEN_JSON: &str = include_str!("../examples/users.json");
const EXAMPLE_SCHEMA: &str = include_str!("../examples/users.kiln");
const GOLDEN_JSONL: &str = include_str!("../examples/users.jsonl");

// Same fields as the example, different seed: edge generation must not change.
const EXAMPLE_SCHEMA_SEED_SWAP: &str = r#"version = 1
seed = 7
count = 4
format = json

record User {
  id: int(1..9999)
  name: enum(Alice, Bob, Cara, Deniz)
  active: bool
  score: float(0..10000)
  handle: string("u_", 3..6)
  tags: list(enum(red, green, blue), 0..3)
}
"#;

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
fn jsonl_matches_the_golden_fixture() {
    let output = kiln()
        .args(["generate", "examples/users.kiln", "--format", "jsonl"])
        .output()
        .unwrap();
    assert!(output.status.success(), "{}", stderr(&output));
    assert_eq!(stdout(&output).trim_end(), GOLDEN_JSONL.trim_end());
}

#[test]
fn edges_mode_is_reproducible_and_ignores_the_seed() {
    let first = kiln().args(["generate", "examples/users.kiln", "--edges"]).output().unwrap();
    let schema_path = write_temp("edge-seed", "kiln", EXAMPLE_SCHEMA_SEED_SWAP);
    let second = kiln().arg("generate").arg(&schema_path).arg("--edges").output().unwrap();
    cleanup(&schema_path);
    assert!(first.status.success());
    assert!(second.status.success());
    assert_eq!(stdout(&first), stdout(&second));
    assert!(stdout(&first).contains("\"id\": 1,"));
    assert!(stdout(&first).contains("\"id\": 9999,"));
}

#[test]
fn check_mismatch_prints_a_path_qualified_drift_report() {
    let fixture = write_temp("drift", "json", "{\"count\": 1}\n");
    let output = kiln()
        .args(["check", "examples/users.kiln", "--fixture"])
        .arg(&fixture)
        .output()
        .unwrap();
    cleanup(&fixture);
    assert_eq!(output.status.code(), Some(1));
    assert!(stdout(&output).contains("fixture drift"), "{}", stdout(&output));
    assert!(stdout(&output).contains("$.count"), "{}", stdout(&output));
}

#[test]
fn diff_of_identical_fixtures_exits_zero() {
    let output = kiln()
        .args(["diff", "examples/users.json", "examples/users.json"])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(0));
    assert!(stdout(&output).contains("MATCH"));
}

#[test]
fn diff_of_different_fixtures_reports_paths() {
    let output = kiln()
        .args(["diff", "examples/users.json", "examples/users-edges.json"])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(1));
    assert!(stdout(&output).contains("fixture drift"));
    assert!(stdout(&output).contains("$.rows[0].id"));
}

#[test]
fn manifest_gate_passes_for_all_committed_fixtures() {
    let output = kiln()
        .args(["check", "--manifest", "examples/kiln.toml"])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(0), "{}", stderr(&output));
    assert!(stdout(&output).contains("3/3 fixtures matched"), "{}", stdout(&output));
}

#[test]
fn manifest_gate_aggregates_a_stale_fixture() {
    let stale = write_temp("manifest-stale", "json", "{\"count\": 1}\n");
    let manifest_path = temp_path("manifest", "toml");
    let text = format!(
        "version = 1\nfixture = examples/users.kiln, examples/users.json, json\nfixture = examples/users.kiln, {}, json\n",
        stale.to_string_lossy()
    );
    fs::write(&manifest_path, text).unwrap();
    let output = kiln().arg("check").arg("--manifest").arg(&manifest_path).output().unwrap();
    cleanup(&stale);
    cleanup(&manifest_path);
    assert_eq!(output.status.code(), Some(1), "{}", stdout(&output));
    assert!(stdout(&output).contains("2/2 fixtures matched") || stdout(&output).contains("1/2"), "{}", stdout(&output));
}

#[test]
fn check_json_report_is_machine_readable() {
    let output = kiln()
        .args(["check", "examples/users.kiln", "--fixture", "examples/users.json", "--json"])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(0));
    let text = stdout(&output);
    assert!(text.contains("\"command\":\"check\""), "{}", text);
    assert!(text.contains("\"verdict\":\"PASS\""), "{}", text);
    assert!(text.contains("\"changes\":0"), "{}", text);
}

#[test]
fn diff_json_report_lists_paths() {
    let output = kiln()
        .args(["diff", "examples/users.json", "examples/users-edges.json", "--json"])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(1));
    let text = stdout(&output);
    assert!(text.contains("\"command\":\"diff\""), "{}", text);
    assert!(text.contains("\"path\":\"$.rows[0].id\""), "{}", text);
}

#[test]
fn manifest_json_report_is_a_single_object() {
    let output = kiln()
        .args(["check", "--manifest", "examples/kiln.toml", "--json"])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(0), "{}", stderr(&output));
    let text = stdout(&output);
    assert_eq!(text.lines().count(), 1, "{}", text);
    assert!(text.contains("\"matched\":3,\"total\":3"), "{}", text);
}

#[test]
fn seed_override_changes_generated_bytes() {
    let base = kiln().args(["generate", "examples/users.kiln"]).output().unwrap();
    let changed = kiln()
        .args(["generate", "examples/users.kiln", "--seed", "7"])
        .output()
        .unwrap();
    assert!(base.status.success() && changed.status.success());
    assert_ne!(stdout(&base), stdout(&changed));
    assert_eq!(stdout(&base).trim_end(), GOLDEN_JSON.trim_end());
}

#[test]
fn stdin_schema_produces_the_same_bytes_as_a_file() {
    let mut child = kiln().args(["generate", "-"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    child.stdin.take().unwrap().write_all(EXAMPLE_SCHEMA.as_bytes()).unwrap();
    let output = child.wait_with_output().unwrap();
    assert!(output.status.success(), "{}", stderr(&output));
    assert_eq!(stdout(&output).trim_end(), GOLDEN_JSON.trim_end());
}

#[test]
fn corpus_writes_deterministic_files() {
    let first = std::env::temp_dir().join(format!("kiln-corpus-a-{}", std::process::id()));
    let second = std::env::temp_dir().join(format!("kiln-corpus-b-{}", std::process::id()));
    let _ = fs::remove_dir_all(&first);
    let _ = fs::remove_dir_all(&second);
    let one = kiln().args(["corpus", "examples/users.kiln", "--seeds", "1..3", "--out-dir"])
        .arg(&first).output().unwrap();
    let two = kiln().args(["corpus", "examples/users.kiln", "--seeds", "1..3", "--out-dir"])
        .arg(&second).output().unwrap();
    assert!(one.status.success(), "{}", stderr(&one));
    assert!(two.status.success(), "{}", stderr(&two));
    let names: Vec<String> = fs::read_dir(&first).unwrap()
        .map(|entry| entry.unwrap().file_name().to_string_lossy().to_string()).collect();
    assert_eq!(names.len(), 3);
    assert!(names.contains(&"user_1.json".to_string()));
    assert!(names.contains(&"user_3.json".to_string()));
    for name in &names {
        assert_eq!(
            fs::read_to_string(first.join(name)).unwrap(),
            fs::read_to_string(second.join(name)).unwrap()
        );
    }
    let _ = fs::remove_dir_all(&first);
    let _ = fs::remove_dir_all(&second);
}

#[test]
fn corpus_rejects_an_oversized_range() {
    let dir = std::env::temp_dir().join(format!("kiln-corpus-bad-{}", std::process::id()));
    let output = kiln().args(["corpus", "examples/users.kiln", "--seeds", "1..1002", "--out-dir"])
        .arg(&dir).output().unwrap();
    assert_eq!(output.status.code(), Some(2));
    assert!(stderr(&output).contains("Why:"));
}

#[test]
fn fields_command_lists_schema_details() {
    let output = kiln().args(["fields", "examples/users.kiln"]).output().unwrap();
    assert!(output.status.success(), "{}", stderr(&output));
    let text = stdout(&output);
    assert!(text.contains("schema: User"), "{}", text);
    assert!(text.contains("1..9999"), "{}", text);
    assert!(text.contains("tags"), "{}", text);
}

#[test]
fn fields_json_is_machine_readable() {
    let output = kiln().args(["fields", "examples/users.kiln", "--json"]).output().unwrap();
    assert!(output.status.success());
    let text = stdout(&output);
    assert!(text.contains("\"record\":\"User\""), "{}", text);
    assert!(text.contains("\"name\":\"handle\""), "{}", text);
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
