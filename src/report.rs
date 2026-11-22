//! Pure machine-readable reports. Stable key order, versioned schema.

use crate::drift::Change;
use crate::model::{Format, Verdict};

pub fn quoted(value: &str) -> String {
    let mut out = String::from("\"");
    for c in value.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

fn kind_label(kind: &crate::drift::ChangeKind) -> &'static str {
    match kind {
        crate::drift::ChangeKind::Added => "added",
        crate::drift::ChangeKind::Removed => "removed",
        crate::drift::ChangeKind::Changed => "changed",
    }
}

fn change_object(change: &Change) -> String {
    format!(
        "{{\"path\":{},\"kind\":{},\"expected\":{},\"actual\":{}}}",
        quoted(&change.path),
        quoted(kind_label(&change.kind)),
        quoted(&change.expected),
        quoted(&change.actual)
    )
}

/// Result of a single fixture check.
pub fn render_check(verdict: Verdict, changes: usize, format: Format) -> String {
    format!(
        "{{\"schema\":1,\"command\":\"check\",\"verdict\":{},\"changes\":{},\"format\":{}}}\n",
        quoted(verdict.label()),
        changes,
        quoted(format.label())
    )
}

/// Result of comparing two committed fixtures. Changes are ordered as the
/// structural differ produced them.
pub fn render_diff(verdict: Verdict, changes: &[Change]) -> String {
    let items: Vec<String> = changes.iter().map(change_object).collect();
    format!(
        "{{\"schema\":1,\"command\":\"diff\",\"verdict\":{},\"changes\":[{}]}}\n",
        quoted(verdict.label()),
        items.join(",")
    )
}

/// Aggregate result of a manifest run, with one verdict per entry.
pub fn render_manifest(verdicts: &[Verdict]) -> String {
    let items: Vec<String> = verdicts
        .iter()
        .map(|verdict| quoted(verdict.label()))
        .collect();
    let matched = verdicts.iter().filter(|verdict| **verdict == Verdict::Pass).count();
    let verdict = if verdicts.iter().all(|value| *value == Verdict::Pass) {
        Verdict::Pass
    } else {
        Verdict::Fail
    };
    format!(
        "{{\"schema\":1,\"command\":\"manifest\",\"verdict\":{},\"matched\":{},\"total\":{},\"entries\":[{}]}}\n",
        quoted(verdict.label()),
        matched,
        verdicts.len(),
        items.join(",")
    )
}
