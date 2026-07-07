use assert_cmd::Command;
use predicates::prelude::*;
use std::fs;
use tempfile::TempDir;

fn damask() -> Command {
    let mut cmd = Command::cargo_bin("damask").unwrap();
    // Hermetic: tests must not inherit a live Claude Code session (or
    // damask identity vars) from the environment running cargo test —
    // init auto-scaffolds and writes stamp provenance from these.
    for var in [
        "CLAUDECODE",
        "CLAUDE_CODE_SESSION_ID",
        "CLAUDE_SESSION_ID",
        "DAMASK_AGENT",
        "DAMASK_SESSION",
        "DAMASK_NS",
    ] {
        cmd.env_remove(var);
    }
    cmd
}

fn init_project(dir: &TempDir) -> &TempDir {
    damask()
        .arg("init")
        .current_dir(dir.path())
        .assert()
        .success()
        .stdout(predicate::str::contains("Initialized .damask/"));
    dir
}

fn set_ns(dir: &TempDir, ns: &str) {
    damask()
        .args(["ns", "set", ns])
        .current_dir(dir.path())
        .assert()
        .success();
}

#[test]
fn init_creates_damask_directory() {
    let dir = TempDir::new().unwrap();
    init_project(&dir);

    assert!(dir.path().join(".damask").is_dir());
    assert!(dir.path().join(".damask/edges").is_dir());
    assert!(dir.path().join(".damask/config.json").is_file());
    assert!(dir.path().join(".damask/.gitignore").is_file());
}

#[test]
fn init_rejects_duplicate() {
    let dir = TempDir::new().unwrap();
    init_project(&dir);

    damask()
        .arg("init")
        .current_dir(dir.path())
        .assert()
        .failure();
}

#[test]
fn ns_set_and_list() {
    let dir = TempDir::new().unwrap();
    init_project(&dir);

    set_ns(&dir, "security-audit");

    // Create a file and span so the namespace JSONL file exists
    fs::write(dir.path().join("test.rs"), "x\n").unwrap();
    damask()
        .args(["span", "test.rs", "1", "1"])
        .current_dir(dir.path())
        .assert()
        .success();

    damask()
        .args(["ns", "list"])
        .current_dir(dir.path())
        .assert()
        .success()
        .stdout(predicate::str::contains("security-audit"));
}

#[test]
fn span_creates_fact() {
    let dir = TempDir::new().unwrap();
    init_project(&dir);
    set_ns(&dir, "test");

    // Create a source file
    fs::write(
        dir.path().join("hello.rs"),
        "fn main() {\n    println!(\"hello\");\n}\n",
    )
    .unwrap();

    let output = damask()
        .args(["span", "hello.rs", "1", "3"])
        .current_dir(dir.path())
        .output()
        .unwrap();

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("hello.rs:1-3"));
    assert!(stdout.contains("s_"));

    // Verify JSONL file exists with content
    let jsonl = fs::read_to_string(dir.path().join(".damask/edges/test.jsonl")).unwrap();
    assert!(jsonl.contains("\"t\":\"span\""));
    assert!(jsonl.contains("hello.rs"));
}

#[test]
fn span_json_format() {
    let dir = TempDir::new().unwrap();
    init_project(&dir);
    set_ns(&dir, "test");

    fs::write(dir.path().join("test.rs"), "let x = 1;\nlet y = 2;\n").unwrap();

    damask()
        .args(["--format", "json", "span", "test.rs", "1", "2"])
        .current_dir(dir.path())
        .assert()
        .success()
        .stdout(predicate::str::contains("\"path\": \"test.rs\""))
        .stdout(predicate::str::contains("\"content_hash\""));
}

#[test]
fn span_rejects_invalid_range() {
    let dir = TempDir::new().unwrap();
    init_project(&dir);
    set_ns(&dir, "test");

    damask()
        .args(["span", "test.rs", "5", "1"])
        .current_dir(dir.path())
        .assert()
        .failure()
        .stderr(predicate::str::contains("start line"));
}

#[test]
fn edge_creates_fact() {
    let dir = TempDir::new().unwrap();
    init_project(&dir);
    set_ns(&dir, "test");

    fs::write(dir.path().join("test.rs"), "fn foo() {}\n").unwrap();

    // Create a span first
    let output = damask()
        .args(["--format", "json", "span", "test.rs", "1", "1"])
        .current_dir(dir.path())
        .output()
        .unwrap();
    let stdout = String::from_utf8(output.stdout).unwrap();
    let span: serde_json::Value = serde_json::from_str(&stdout).unwrap();
    let span_id = span["id"].as_str().unwrap();

    // Create an edge
    damask()
        .args([
            "edge",
            span_id,
            "_",
            "risk",
            r#"{"summary":"test risk","confidence":0.9}"#,
        ])
        .current_dir(dir.path())
        .assert()
        .success()
        .stdout(predicate::str::contains("e_"))
        .stdout(predicate::str::contains("[risk]"));

    // Verify JSONL has both span and edge
    let jsonl = fs::read_to_string(dir.path().join(".damask/edges/test.jsonl")).unwrap();
    let lines: Vec<&str> = jsonl.trim().lines().collect();
    assert_eq!(lines.len(), 2);
    assert!(lines[1].contains("\"t\":\"edge\""));
    assert!(lines[1].contains("\"rel\":\"risk\""));
}

#[test]
fn edge_rejects_invalid_id() {
    let dir = TempDir::new().unwrap();
    init_project(&dir);
    set_ns(&dir, "test");

    damask()
        .args(["edge", "garbage", "_", "risk"])
        .current_dir(dir.path())
        .assert()
        .failure()
        .stderr(predicate::str::contains("not a valid span or edge ID"));
}

#[test]
fn init_default_namespace_makes_first_write_succeed() {
    // init writes a default namespace (sanitized repo dir name) so the
    // first write succeeds without a `ns set` ritual.
    let dir = TempDir::new().unwrap();
    init_project(&dir);
    // Don't set a namespace — the default from init must carry the write.

    fs::write(dir.path().join("test.rs"), "x\n").unwrap();

    damask()
        .args(["span", "test.rs", "1", "1"])
        .current_dir(dir.path())
        .assert()
        .success();

    // The write landed in the config default_ns.
    let config = fs::read_to_string(dir.path().join(".damask/config.json")).unwrap();
    let doc: serde_json::Value = serde_json::from_str(&config).unwrap();
    let ns = doc["default_ns"].as_str().expect("init must set default_ns");
    assert!(
        dir.path().join(format!(".damask/edges/{ns}.jsonl")).is_file(),
        "span must land in the default namespace"
    );
}

#[test]
fn ns_override_flag() {
    let dir = TempDir::new().unwrap();
    init_project(&dir);

    fs::write(dir.path().join("test.rs"), "x\n").unwrap();

    damask()
        .args(["--ns", "override-ns", "span", "test.rs", "1", "1"])
        .current_dir(dir.path())
        .assert()
        .success();

    assert!(dir.path().join(".damask/edges/override-ns.jsonl").exists());
}

#[test]
fn tui_requires_project() {
    let dir = TempDir::new().unwrap();

    // TUI should fail gracefully without a .damask/ directory
    damask()
        .arg("tui")
        .current_dir(dir.path())
        .assert()
        .failure()
        .stderr(predicate::str::contains(".damask"));
}

#[test]
fn at_shows_ranked_edges() {
    let dir = TempDir::new().unwrap();
    init_project(&dir);
    set_ns(&dir, "test");

    fs::write(
        dir.path().join("auth.rs"),
        "fn validate() {\n    // check\n}\n",
    )
    .unwrap();

    // Create span
    let output = damask()
        .args(["--format", "json", "span", "auth.rs", "1", "3"])
        .current_dir(dir.path())
        .output()
        .unwrap();
    let span: serde_json::Value =
        serde_json::from_str(&String::from_utf8(output.stdout).unwrap()).unwrap();
    let span_id = span["id"].as_str().unwrap();

    // Create edges with different ranking signals
    damask()
        .args([
            "edge",
            span_id,
            "_",
            "risk",
            r#"{"summary":"High risk","confidence":0.95,"action":"Fix it"}"#,
        ])
        .current_dir(dir.path())
        .assert()
        .success();

    damask()
        .args([
            "edge",
            span_id,
            "_",
            "describes",
            r#"{"summary":"A function"}"#,
        ])
        .current_dir(dir.path())
        .assert()
        .success();

    // Test `at` human output
    damask()
        .args(["at", "auth.rs:2"])
        .current_dir(dir.path())
        .assert()
        .success()
        .stdout(predicate::str::contains("High risk"))
        .stdout(predicate::str::contains("A function"))
        .stdout(predicate::str::contains("2 edges shown"));

    // Test `at` JSON output
    let output = damask()
        .args(["--format", "json", "at", "auth.rs:2"])
        .current_dir(dir.path())
        .output()
        .unwrap();
    let json: serde_json::Value =
        serde_json::from_str(&String::from_utf8(output.stdout).unwrap()).unwrap();
    let edges = json["edges"].as_array().unwrap();
    assert_eq!(edges.len(), 2);
    // Risk should rank first
    assert_eq!(edges[0]["rel"], "risk");
    assert!(edges[0]["score"].as_f64().unwrap() > edges[1]["score"].as_f64().unwrap());
}

#[test]
fn at_no_spans() {
    let dir = TempDir::new().unwrap();
    init_project(&dir);

    damask()
        .args(["at", "nonexistent.rs:1"])
        .current_dir(dir.path())
        .assert()
        .success()
        .stdout(predicate::str::contains("No spans"));
}

#[test]
fn where_filters_by_rel() {
    let dir = TempDir::new().unwrap();
    init_project(&dir);
    set_ns(&dir, "test");

    fs::write(dir.path().join("test.rs"), "fn foo() {}\nfn bar() {}\n").unwrap();

    // Create a span and two edges with different rels
    let output = damask()
        .args(["--format", "json", "span", "test.rs", "1", "2"])
        .current_dir(dir.path())
        .output()
        .unwrap();
    let span: serde_json::Value =
        serde_json::from_str(&String::from_utf8(output.stdout).unwrap()).unwrap();
    let span_id = span["id"].as_str().unwrap();

    damask()
        .args([
            "edge",
            span_id,
            "_",
            "risk",
            r#"{"summary":"A risk","confidence":0.9}"#,
        ])
        .current_dir(dir.path())
        .assert()
        .success();

    damask()
        .args([
            "edge",
            span_id,
            "_",
            "describes",
            r#"{"summary":"A description"}"#,
        ])
        .current_dir(dir.path())
        .assert()
        .success();

    // Filter by rel=risk — should show only the risk edge
    damask()
        .args(["where", "rel=risk"])
        .current_dir(dir.path())
        .assert()
        .success()
        .stdout(predicate::str::contains("A risk"))
        .stdout(predicate::str::contains("1 edges matching"));

    // JSON output
    let output = damask()
        .args(["--format", "json", "where", "rel=risk"])
        .current_dir(dir.path())
        .output()
        .unwrap();
    let json: serde_json::Value =
        serde_json::from_str(&String::from_utf8(output.stdout).unwrap()).unwrap();
    let edges = json["edges"].as_array().unwrap();
    assert_eq!(edges.len(), 1);
    assert_eq!(edges[0]["rel"], "risk");
}

#[test]
fn record_flag_form_succeeds_and_sloppy_forms_teach() {
    // The write path a weak model guesses must succeed on attempt 1, and
    // every classic failure must be a teaching error, not silent poison.
    let dir = TempDir::new().unwrap();
    init_project(&dir);
    set_ns(&dir, "test");
    fs::write(dir.path().join("f.rs"), "a\nb\nc\nd\ne\nf\n").unwrap();

    // The guessable form: flags, no JSON.
    let output = damask()
        .args([
            "--format", "json", "record", "f.rs", "1", "3", "risk",
            "-m", "token never expires", "-c", "0.9",
            "--action", "add expiry", "--tag", "security",
        ])
        .current_dir(dir.path())
        .output()
        .unwrap();
    assert!(output.status.success());
    let jsonl = fs::read_to_string(dir.path().join(".damask/edges/test.jsonl")).unwrap();
    assert!(jsonl.contains("\"summary\":\"token never expires\""));
    assert!(jsonl.contains("\"confidence\":0.9"));
    assert!(jsonl.contains("\"action\":\"add expiry\""));
    assert!(jsonl.contains("\"tags\":[\"security\"]"));

    // -c out of range: clap rejects with a did-you-mean.
    damask()
        .args(["record", "f.rs", "1", "3", "risk", "-m", "x", "-c", "9"])
        .current_dir(dir.path())
        .assert()
        .failure()
        .stderr(predicate::str::contains("did you mean 0.9"));

    // JSON confidence out of range: validated at write time.
    damask()
        .args(["record", "f.rs", "1", "3", "risk", r#"{"summary":"x","confidence":9}"#])
        .current_dir(dir.path())
        .assert()
        .failure()
        .stderr(predicate::str::contains("did you mean 0.9"));

    // String confidence: previously vanished from numeric predicates.
    damask()
        .args(["record", "f.rs", "1", "3", "risk", r#"{"summary":"x","confidence":"0.9"}"#])
        .current_dir(dir.path())
        .assert()
        .failure()
        .stderr(predicate::str::contains("remove the quotes"));

    // Hallucinated line range: previously created a dead span silently.
    damask()
        .args(["record", "f.rs", "100", "120", "risk", "-m", "x"])
        .current_dir(dir.path())
        .assert()
        .failure()
        .stderr(predicate::str::contains("past the end of f.rs (6 lines)"));

    // Broken JSON teaches the flag form.
    damask()
        .args(["record", "f.rs", "1", "3", "risk", "{summary: 'oops'}"])
        .current_dir(dir.path())
        .assert()
        .failure()
        .stderr(predicate::str::contains("-m \"what you found\" -c 0.9"));

    // No payload teaches the shortest correct invocation.
    damask()
        .args(["record", "f.rs", "1", "3", "risk"])
        .current_dir(dir.path())
        .assert()
        .failure()
        .stderr(predicate::str::contains("damask record f.rs 1 3 risk -m"));
}

#[test]
fn batch_validates_payloads() {
    let dir = TempDir::new().unwrap();
    init_project(&dir);
    set_ns(&dir, "test");
    fs::write(dir.path().join("f.rs"), "a\nb\n").unwrap();

    let batch = r#"[{"span":{"path":"f.rs","start":1,"end":2}},{"edge":{"from":"$0","to":"_","rel":"risk","payload":{"summary":"x","confidence":9}}}]"#;
    damask()
        .args(["batch", "--stdin"])
        .write_stdin(batch)
        .current_dir(dir.path())
        .assert()
        .failure()
        .stderr(predicate::str::contains("item 1"))
        .stderr(predicate::str::contains("did you mean 0.9"));

    // Nothing should have been written (atomic batch).
    let jsonl = fs::read_to_string(dir.path().join(".damask/edges/test.jsonl"))
        .unwrap_or_default();
    assert!(!jsonl.contains("confidence"), "failed batch must write nothing");
}

#[test]
fn where_output_is_ranked_and_located() {
    // The triage surface must be actionable per row: anchor file:line in
    // human output, score + span object in JSON, ranked order by default.
    let dir = TempDir::new().unwrap();
    init_project(&dir);
    set_ns(&dir, "test");

    fs::write(dir.path().join("live.rs"), "fn live() {}\nfn more() {}\n").unwrap();

    let record = |file: &str, summary: &str| {
        damask()
            .args([
                "record",
                file,
                "1",
                "2",
                "risk",
                &format!(r#"{{"summary":"{summary}","confidence":0.9}}"#),
            ])
            .current_dir(dir.path())
            .assert()
            .success();
    };
    record("live.rs", "live finding");

    // A second edge anchored to a file that then disappears → resolution
    // becomes missing → must sink below the live finding under ranking.
    fs::write(dir.path().join("doomed.rs"), "fn doomed() {}\nfn gone() {}\n").unwrap();
    record("doomed.rs", "doomed finding");
    fs::remove_file(dir.path().join("doomed.rs")).unwrap();

    // Human: anchor location on the edge line.
    damask()
        .args(["where", "rel=risk"])
        .current_dir(dir.path())
        .assert()
        .success()
        .stdout(predicate::str::contains("live.rs:1-2"));

    // JSON: score present, span object carries path + resolution.
    let output = damask()
        .args(["--format", "json", "where", "rel=risk"])
        .current_dir(dir.path())
        .output()
        .unwrap();
    let json: serde_json::Value =
        serde_json::from_str(&String::from_utf8(output.stdout).unwrap()).unwrap();
    let edges = json["edges"].as_array().unwrap();
    assert_eq!(edges.len(), 2);
    // Ranked: the live-anchored finding outranks the missing-anchored one.
    assert_eq!(edges[0]["span"]["path"], "live.rs");
    assert!(edges[0]["score"].as_f64().unwrap() > edges[1]["score"].as_f64().unwrap());
    assert_eq!(edges[1]["span"]["path"], "doomed.rs");
    assert_eq!(edges[1]["span"]["resolution"], "missing");

    // --sort ts flips to newest-first regardless of freshness.
    let output = damask()
        .args(["--format", "json", "where", "rel=risk", "--sort", "ts"])
        .current_dir(dir.path())
        .output()
        .unwrap();
    let json: serde_json::Value =
        serde_json::from_str(&String::from_utf8(output.stdout).unwrap()).unwrap();
    let edges = json["edges"].as_array().unwrap();
    assert_eq!(edges[0]["span"]["path"], "doomed.rs", "newest first under --sort ts");
}

#[test]
fn where_filters_by_confidence() {
    let dir = TempDir::new().unwrap();
    init_project(&dir);
    set_ns(&dir, "test");

    fs::write(dir.path().join("test.rs"), "x\n").unwrap();

    let output = damask()
        .args(["--format", "json", "span", "test.rs", "1", "1"])
        .current_dir(dir.path())
        .output()
        .unwrap();
    let span: serde_json::Value =
        serde_json::from_str(&String::from_utf8(output.stdout).unwrap()).unwrap();
    let span_id = span["id"].as_str().unwrap();

    damask()
        .args([
            "edge",
            span_id,
            "_",
            "risk",
            r#"{"summary":"High conf","confidence":0.95}"#,
        ])
        .current_dir(dir.path())
        .assert()
        .success();

    damask()
        .args([
            "edge",
            span_id,
            "_",
            "risk",
            r#"{"summary":"Low conf","confidence":0.3}"#,
        ])
        .current_dir(dir.path())
        .assert()
        .success();

    let output = damask()
        .args(["--format", "json", "where", "confidence>0.8"])
        .current_dir(dir.path())
        .output()
        .unwrap();
    let json: serde_json::Value =
        serde_json::from_str(&String::from_utf8(output.stdout).unwrap()).unwrap();
    let edges = json["edges"].as_array().unwrap();
    assert_eq!(edges.len(), 1);
    assert_eq!(edges[0]["payload"]["summary"], "High conf");
}

#[test]
fn where_no_matches() {
    let dir = TempDir::new().unwrap();
    init_project(&dir);
    set_ns(&dir, "test");

    fs::write(dir.path().join("test.rs"), "x\n").unwrap();

    damask()
        .args(["span", "test.rs", "1", "1"])
        .current_dir(dir.path())
        .assert()
        .success();

    damask()
        .args(["where", "rel=nonexistent"])
        .current_dir(dir.path())
        .assert()
        .success()
        .stdout(predicate::str::contains("0 edges matching"));
}

#[test]
fn follow_traverses_edges() {
    let dir = TempDir::new().unwrap();
    init_project(&dir);
    set_ns(&dir, "test");

    fs::write(dir.path().join("a.rs"), "fn a() {}\n").unwrap();
    fs::write(dir.path().join("b.rs"), "fn b() {}\n").unwrap();

    // Create two spans
    let output_a = damask()
        .args(["--format", "json", "span", "a.rs", "1", "1"])
        .current_dir(dir.path())
        .output()
        .unwrap();
    let span_a: serde_json::Value =
        serde_json::from_str(&String::from_utf8(output_a.stdout).unwrap()).unwrap();
    let id_a = span_a["id"].as_str().unwrap();

    let output_b = damask()
        .args(["--format", "json", "span", "b.rs", "1", "1"])
        .current_dir(dir.path())
        .output()
        .unwrap();
    let span_b: serde_json::Value =
        serde_json::from_str(&String::from_utf8(output_b.stdout).unwrap()).unwrap();
    let id_b = span_b["id"].as_str().unwrap();

    // Create edge from a to b
    damask()
        .args([
            "edge",
            id_a,
            id_b,
            "depends_on",
            r#"{"summary":"A depends on B"}"#,
        ])
        .current_dir(dir.path())
        .assert()
        .success();

    // Create a leaf edge on a
    damask()
        .args([
            "edge",
            id_a,
            "_",
            "risk",
            r#"{"summary":"Risk on A","confidence":0.9}"#,
        ])
        .current_dir(dir.path())
        .assert()
        .success();

    // Follow from a — should show both edges
    damask()
        .args(["follow", id_a])
        .current_dir(dir.path())
        .assert()
        .success()
        .stdout(predicate::str::contains("depends_on"))
        .stdout(predicate::str::contains("risk"))
        .stdout(predicate::str::contains("b.rs"));

    // Follow with rel filter
    damask()
        .args(["follow", id_a, "risk"])
        .current_dir(dir.path())
        .assert()
        .success()
        .stdout(predicate::str::contains("risk"))
        .stdout(predicate::str::contains("Risk on A"));

    // Follow JSON output
    let output = damask()
        .args(["--format", "json", "follow", id_a])
        .current_dir(dir.path())
        .output()
        .unwrap();
    let json: serde_json::Value =
        serde_json::from_str(&String::from_utf8(output.stdout).unwrap()).unwrap();
    assert_eq!(json["id"].as_str().unwrap(), id_a);
    let children = json["children"].as_array().unwrap();
    assert_eq!(children.len(), 2);
}

#[test]
fn follow_rejects_invalid_id() {
    let dir = TempDir::new().unwrap();
    init_project(&dir);

    damask()
        .args(["follow", "garbage"])
        .current_dir(dir.path())
        .assert()
        .failure()
        .stderr(predicate::str::contains("not a valid span or edge ID"));
}

#[test]
fn endorse_creates_meta_edge() {
    let dir = TempDir::new().unwrap();
    init_project(&dir);
    set_ns(&dir, "test");

    fs::write(dir.path().join("test.rs"), "fn foo() {}\n").unwrap();

    // Create span and edge
    let output = damask()
        .args(["--format", "json", "span", "test.rs", "1", "1"])
        .current_dir(dir.path())
        .output()
        .unwrap();
    let span: serde_json::Value =
        serde_json::from_str(&String::from_utf8(output.stdout).unwrap()).unwrap();
    let span_id = span["id"].as_str().unwrap();

    let output = damask()
        .args([
            "--format",
            "json",
            "edge",
            span_id,
            "_",
            "risk",
            r#"{"summary":"A risk","confidence":0.9}"#,
        ])
        .current_dir(dir.path())
        .output()
        .unwrap();
    let edge: serde_json::Value =
        serde_json::from_str(&String::from_utf8(output.stdout).unwrap()).unwrap();
    let edge_id = edge["id"].as_str().unwrap();

    // Endorse the edge
    damask()
        .args([
            "endorse",
            edge_id,
            r#"{"summary":"Confirmed during review"}"#,
        ])
        .current_dir(dir.path())
        .assert()
        .success()
        .stdout(predicate::str::contains("Endorsed"));

    // Verify the JSONL file has the endorsement
    let jsonl = fs::read_to_string(dir.path().join(".damask/edges/test.jsonl")).unwrap();
    assert!(jsonl.contains("\"rel\":\"endorsed\""));
}

#[test]
fn dispute_creates_meta_edge() {
    let dir = TempDir::new().unwrap();
    init_project(&dir);
    set_ns(&dir, "test");

    fs::write(dir.path().join("test.rs"), "fn foo() {}\n").unwrap();

    let output = damask()
        .args(["--format", "json", "span", "test.rs", "1", "1"])
        .current_dir(dir.path())
        .output()
        .unwrap();
    let span: serde_json::Value =
        serde_json::from_str(&String::from_utf8(output.stdout).unwrap()).unwrap();
    let span_id = span["id"].as_str().unwrap();

    let output = damask()
        .args([
            "--format",
            "json",
            "edge",
            span_id,
            "_",
            "risk",
            r#"{"summary":"A risk"}"#,
        ])
        .current_dir(dir.path())
        .output()
        .unwrap();
    let edge: serde_json::Value =
        serde_json::from_str(&String::from_utf8(output.stdout).unwrap()).unwrap();
    let edge_id = edge["id"].as_str().unwrap();

    // Dispute the edge (payload required)
    damask()
        .args([
            "dispute",
            edge_id,
            r#"{"summary":"This was fixed in commit abc123"}"#,
        ])
        .current_dir(dir.path())
        .assert()
        .success()
        .stdout(predicate::str::contains("Disputed"));

    let jsonl = fs::read_to_string(dir.path().join(".damask/edges/test.jsonl")).unwrap();
    assert!(jsonl.contains("\"rel\":\"disputed\""));
}

#[test]
fn status_shows_project_health() {
    let dir = TempDir::new().unwrap();
    init_project(&dir);
    set_ns(&dir, "test");

    fs::write(dir.path().join("test.rs"), "fn foo() {}\n").unwrap();

    damask()
        .args(["span", "test.rs", "1", "1"])
        .current_dir(dir.path())
        .assert()
        .success();

    damask()
        .arg("status")
        .current_dir(dir.path())
        .assert()
        .success()
        .stdout(predicate::str::contains("Damask status"))
        .stdout(predicate::str::contains("Spans:"));
}

#[test]
fn lint_flags_empty_payload() {
    let dir = TempDir::new().unwrap();
    init_project(&dir);
    set_ns(&dir, "test");

    fs::write(dir.path().join("test.rs"), "fn foo() {}\n").unwrap();

    let output = damask()
        .args(["--format", "json", "span", "test.rs", "1", "1"])
        .current_dir(dir.path())
        .output()
        .unwrap();
    let span: serde_json::Value =
        serde_json::from_str(&String::from_utf8(output.stdout).unwrap()).unwrap();
    let span_id = span["id"].as_str().unwrap();

    // Create edge with empty payload
    damask()
        .args(["edge", span_id, "_", "risk"])
        .current_dir(dir.path())
        .assert()
        .success();

    // Lint should flag empty payload
    damask()
        .arg("lint")
        .current_dir(dir.path())
        .assert()
        .success()
        .stdout(predicate::str::contains("empty payload"));
}

#[test]
fn lint_clean_project() {
    let dir = TempDir::new().unwrap();
    init_project(&dir);
    set_ns(&dir, "test");

    fs::write(dir.path().join("test.rs"), "fn foo() {}\n").unwrap();

    let output = damask()
        .args(["--format", "json", "span", "test.rs", "1", "1"])
        .current_dir(dir.path())
        .output()
        .unwrap();
    let span: serde_json::Value =
        serde_json::from_str(&String::from_utf8(output.stdout).unwrap()).unwrap();
    let span_id = span["id"].as_str().unwrap();

    // Create a well-formed edge
    damask()
        .args([
            "edge",
            span_id,
            "_",
            "risk",
            r#"{"summary":"A proper risk","confidence":0.9,"action":"Fix it"}"#,
        ])
        .current_dir(dir.path())
        .assert()
        .success();

    // Lint should pass clean
    damask()
        .arg("lint")
        .current_dir(dir.path())
        .assert()
        .success()
        .stdout(predicate::str::contains("No lint issues"));
}

#[test]
fn log_shows_fact_log() {
    let dir = TempDir::new().unwrap();
    init_project(&dir);
    set_ns(&dir, "test");

    fs::write(dir.path().join("test.rs"), "fn foo() {}\n").unwrap();

    let output = damask()
        .args(["--format", "json", "span", "test.rs", "1", "1"])
        .current_dir(dir.path())
        .output()
        .unwrap();
    let span: serde_json::Value =
        serde_json::from_str(&String::from_utf8(output.stdout).unwrap()).unwrap();
    let span_id = span["id"].as_str().unwrap();

    damask()
        .args([
            "edge",
            span_id,
            "_",
            "risk",
            r#"{"summary":"A risk","confidence":0.9}"#,
        ])
        .current_dir(dir.path())
        .assert()
        .success();

    damask()
        .arg("log")
        .current_dir(dir.path())
        .assert()
        .success()
        .stdout(predicate::str::contains("Fact log"))
        .stdout(predicate::str::contains("span"))
        .stdout(predicate::str::contains("edge"));
}

#[test]
fn review_shows_active_edges() {
    let dir = TempDir::new().unwrap();
    init_project(&dir);
    set_ns(&dir, "test");

    fs::write(dir.path().join("test.rs"), "fn foo() {}\n").unwrap();

    let output = damask()
        .args(["--format", "json", "span", "test.rs", "1", "1"])
        .current_dir(dir.path())
        .output()
        .unwrap();
    let span: serde_json::Value =
        serde_json::from_str(&String::from_utf8(output.stdout).unwrap()).unwrap();
    let span_id = span["id"].as_str().unwrap();

    damask()
        .args([
            "edge",
            span_id,
            "_",
            "risk",
            r#"{"summary":"Review this","confidence":0.8}"#,
        ])
        .current_dir(dir.path())
        .assert()
        .success();

    damask()
        .arg("review")
        .current_dir(dir.path())
        .assert()
        .success()
        .stdout(predicate::str::contains("Review:"))
        .stdout(predicate::str::contains("Review this"));
}

#[test]
fn resolve_materializes_content() {
    let dir = TempDir::new().unwrap();
    init_project(&dir);
    set_ns(&dir, "test");

    fs::write(
        dir.path().join("hello.rs"),
        "fn main() {\n    println!(\"hello\");\n}\n",
    )
    .unwrap();

    let output = damask()
        .args(["--format", "json", "span", "hello.rs", "1", "3"])
        .current_dir(dir.path())
        .output()
        .unwrap();
    let span: serde_json::Value =
        serde_json::from_str(&String::from_utf8(output.stdout).unwrap()).unwrap();
    let span_id = span["id"].as_str().unwrap();

    damask()
        .args(["resolve", span_id])
        .current_dir(dir.path())
        .assert()
        .success()
        .stdout(predicate::str::contains("Resolve:"))
        .stdout(predicate::str::contains("fn main()"))
        .stdout(predicate::str::contains("Resolution:"));
}

#[test]
fn compact_removes_inactive_edges() {
    let dir = TempDir::new().unwrap();
    init_project(&dir);
    set_ns(&dir, "test");

    fs::write(dir.path().join("test.rs"), "fn foo() {}\n").unwrap();

    let output = damask()
        .args(["--format", "json", "span", "test.rs", "1", "1"])
        .current_dir(dir.path())
        .output()
        .unwrap();
    let span: serde_json::Value =
        serde_json::from_str(&String::from_utf8(output.stdout).unwrap()).unwrap();
    let span_id = span["id"].as_str().unwrap();

    // Create two edges — old and new — then supersede old
    let output1 = damask()
        .args([
            "--format",
            "json",
            "edge",
            span_id,
            "_",
            "risk",
            r#"{"summary":"Old risk"}"#,
        ])
        .current_dir(dir.path())
        .output()
        .unwrap();
    let old_edge: serde_json::Value =
        serde_json::from_str(&String::from_utf8(output1.stdout).unwrap()).unwrap();
    let old_id = old_edge["id"].as_str().unwrap();

    let output2 = damask()
        .args([
            "--format",
            "json",
            "edge",
            span_id,
            "_",
            "risk",
            r#"{"summary":"New risk"}"#,
        ])
        .current_dir(dir.path())
        .output()
        .unwrap();
    let new_edge: serde_json::Value =
        serde_json::from_str(&String::from_utf8(output2.stdout).unwrap()).unwrap();
    let new_id = new_edge["id"].as_str().unwrap();

    // Supersede old with new
    damask()
        .args(["edge", new_id, old_id, "supersedes"])
        .current_dir(dir.path())
        .assert()
        .success();

    // Compact should remove inactive edges
    damask()
        .arg("compact")
        .current_dir(dir.path())
        .assert()
        .success()
        .stdout(predicate::str::contains("Compact:"))
        .stdout(predicate::str::contains("Removed:"));
}

#[test]
fn why_shows_provenance() {
    let dir = TempDir::new().unwrap();
    init_project(&dir);
    set_ns(&dir, "test");

    fs::write(dir.path().join("test.rs"), "fn foo() {}\n").unwrap();

    let output = damask()
        .args(["--format", "json", "span", "test.rs", "1", "1"])
        .current_dir(dir.path())
        .output()
        .unwrap();
    let span: serde_json::Value =
        serde_json::from_str(&String::from_utf8(output.stdout).unwrap()).unwrap();
    let span_id = span["id"].as_str().unwrap();

    let output = damask()
        .args([
            "--format",
            "json",
            "edge",
            span_id,
            "_",
            "risk",
            r#"{"summary":"A risk"}"#,
        ])
        .current_dir(dir.path())
        .output()
        .unwrap();
    let edge: serde_json::Value =
        serde_json::from_str(&String::from_utf8(output.stdout).unwrap()).unwrap();
    let edge_id = edge["id"].as_str().unwrap();

    damask()
        .args(["why", edge_id])
        .current_dir(dir.path())
        .assert()
        .success()
        .stdout(predicate::str::contains("Edge:"))
        .stdout(predicate::str::contains("A risk"));
}

#[test]
fn blame_shows_history() {
    let dir = TempDir::new().unwrap();
    init_project(&dir);
    set_ns(&dir, "test");

    fs::write(dir.path().join("test.rs"), "fn foo() {}\n").unwrap();

    let output = damask()
        .args(["--format", "json", "span", "test.rs", "1", "1"])
        .current_dir(dir.path())
        .output()
        .unwrap();
    let span: serde_json::Value =
        serde_json::from_str(&String::from_utf8(output.stdout).unwrap()).unwrap();
    let span_id = span["id"].as_str().unwrap();

    damask()
        .args([
            "edge",
            span_id,
            "_",
            "risk",
            r#"{"summary":"A risk","confidence":0.9}"#,
        ])
        .current_dir(dir.path())
        .assert()
        .success();

    damask()
        .args(["blame", span_id])
        .current_dir(dir.path())
        .assert()
        .success()
        .stdout(predicate::str::contains("Blame:"))
        .stdout(predicate::str::contains("Span:"));
}

#[test]
fn ns_merge_rejects_same_namespace() {
    let dir = TempDir::new().unwrap();
    init_project(&dir);

    damask()
        .args(["ns", "merge", "foo", "foo"])
        .current_dir(dir.path())
        .assert()
        .failure()
        .stderr(predicate::str::contains("must be different"));
}

#[test]
fn ns_merge_retags_facts_to_target() {
    let dir = TempDir::new().unwrap();
    init_project(&dir);

    fs::write(dir.path().join("test.rs"), "fn foo() {}\n").unwrap();

    // Create a fact in source-ns
    set_ns(&dir, "source-ns");
    damask()
        .args(["span", "test.rs", "1", "1"])
        .current_dir(dir.path())
        .assert()
        .success();

    // Ensure target-ns exists
    set_ns(&dir, "target-ns");
    damask()
        .args(["span", "test.rs", "1", "1"])
        .current_dir(dir.path())
        .assert()
        .success();

    // Merge source into target
    damask()
        .args(["ns", "merge", "source-ns", "target-ns"])
        .current_dir(dir.path())
        .assert()
        .success();

    // Verify merged facts have ns rewritten to target-ns
    let target_content =
        fs::read_to_string(dir.path().join(".damask/edges/target-ns.jsonl")).unwrap();
    for line in target_content.trim().lines() {
        assert!(
            line.contains("\"ns\":\"target-ns\""),
            "expected ns=target-ns in: {line}"
        );
    }
}

#[test]
fn ns_merge_combines_namespaces() {
    let dir = TempDir::new().unwrap();
    init_project(&dir);

    fs::write(dir.path().join("test.rs"), "fn foo() {}\n").unwrap();

    // Create facts in source namespace
    set_ns(&dir, "source-ns");
    damask()
        .args(["span", "test.rs", "1", "1"])
        .current_dir(dir.path())
        .assert()
        .success();

    // Create target namespace
    set_ns(&dir, "target-ns");
    damask()
        .args(["span", "test.rs", "1", "1"])
        .current_dir(dir.path())
        .assert()
        .success();

    // Merge source into target
    damask()
        .args(["ns", "merge", "source-ns", "target-ns"])
        .current_dir(dir.path())
        .assert()
        .success()
        .stdout(predicate::str::contains("Merged"))
        .stdout(predicate::str::contains("source-ns"));

    // Target JSONL should now have facts from both
    let target_content =
        fs::read_to_string(dir.path().join(".damask/edges/target-ns.jsonl")).unwrap();
    let lines: Vec<&str> = target_content.trim().lines().collect();
    assert_eq!(lines.len(), 2); // original target span + merged source span
}

#[test]
fn non_ascii_payload_does_not_panic() {
    let dir = TempDir::new().unwrap();
    init_project(&dir);
    set_ns(&dir, "test");

    fs::write(dir.path().join("test.rs"), "fn foo() {}\n").unwrap();

    let output = damask()
        .args(["--format", "json", "span", "test.rs", "1", "1"])
        .current_dir(dir.path())
        .output()
        .unwrap();
    let span: serde_json::Value =
        serde_json::from_str(&String::from_utf8(output.stdout).unwrap()).unwrap();
    let span_id = span["id"].as_str().unwrap();

    // Create edge with a long non-ASCII payload (no summary field, so fallback truncation is used)
    let payload = format!(r#"{{"description":"{}"}}"#, "🦀漢字データ".repeat(20));
    damask()
        .args(["edge", span_id, "_", "risk", &payload])
        .current_dir(dir.path())
        .assert()
        .success();

    // `where` uses the summary fallback that truncates payload — should not panic
    damask()
        .args(["where", "rel=risk"])
        .current_dir(dir.path())
        .assert()
        .success()
        .stdout(predicate::str::contains("1 edges matching"));

    // `at` also uses truncation on payloads without summary
    damask()
        .args(["at", "test.rs:1"])
        .current_dir(dir.path())
        .assert()
        .success();
}

#[test]
fn where_ns_filters_by_namespace() {
    let dir = TempDir::new().unwrap();
    init_project(&dir);

    fs::write(dir.path().join("test.rs"), "fn foo() {}\nfn bar() {}\n").unwrap();

    // Create edges in namespace "alpha"
    set_ns(&dir, "alpha");
    let output = damask()
        .args(["--format", "json", "span", "test.rs", "1", "1"])
        .current_dir(dir.path())
        .output()
        .unwrap();
    let span: serde_json::Value =
        serde_json::from_str(&String::from_utf8(output.stdout).unwrap()).unwrap();
    let span_id_a = span["id"].as_str().unwrap().to_string();

    damask()
        .args([
            "edge",
            &span_id_a,
            "_",
            "risk",
            r#"{"summary":"Alpha risk","confidence":0.9}"#,
        ])
        .current_dir(dir.path())
        .assert()
        .success();

    // Create edges in namespace "beta"
    set_ns(&dir, "beta");
    let output = damask()
        .args(["--format", "json", "span", "test.rs", "2", "2"])
        .current_dir(dir.path())
        .output()
        .unwrap();
    let span: serde_json::Value =
        serde_json::from_str(&String::from_utf8(output.stdout).unwrap()).unwrap();
    let span_id_b = span["id"].as_str().unwrap().to_string();

    damask()
        .args([
            "edge",
            &span_id_b,
            "_",
            "risk",
            r#"{"summary":"Beta risk","confidence":0.8}"#,
        ])
        .current_dir(dir.path())
        .assert()
        .success();

    // where --ns alpha should only show alpha edges
    let output = damask()
        .args(["--format", "json", "--ns", "alpha", "where", "rel=risk"])
        .current_dir(dir.path())
        .output()
        .unwrap();
    assert!(output.status.success());
    let json: serde_json::Value =
        serde_json::from_str(&String::from_utf8(output.stdout).unwrap()).unwrap();
    let edges = json["edges"].as_array().unwrap();
    assert_eq!(edges.len(), 1);
    assert_eq!(edges[0]["payload"]["summary"], "Alpha risk");

    // where --ns beta should only show beta edges
    let output = damask()
        .args(["--format", "json", "--ns", "beta", "where", "rel=risk"])
        .current_dir(dir.path())
        .output()
        .unwrap();
    assert!(output.status.success());
    let json: serde_json::Value =
        serde_json::from_str(&String::from_utf8(output.stdout).unwrap()).unwrap();
    let edges = json["edges"].as_array().unwrap();
    assert_eq!(edges.len(), 1);
    assert_eq!(edges[0]["payload"]["summary"], "Beta risk");

    // where without --ns should show both
    let output = damask()
        .args(["--format", "json", "where", "rel=risk"])
        .current_dir(dir.path())
        .output()
        .unwrap();
    assert!(output.status.success());
    let json: serde_json::Value =
        serde_json::from_str(&String::from_utf8(output.stdout).unwrap()).unwrap();
    let edges = json["edges"].as_array().unwrap();
    assert_eq!(edges.len(), 2);
}

#[test]
fn init_claude_creates_scaffolding() {
    let dir = TempDir::new().unwrap();

    damask()
        .args(["init", "--claude"])
        .current_dir(dir.path())
        .assert()
        .success()
        .stdout(predicate::str::contains("Initialized .damask/"))
        .stdout(predicate::str::contains("Claude Code skill synced"));

    // Verify skill file created
    assert!(dir.path().join(".claude/skills/damask/SKILL.md").is_file());

    // Verify settings.json created with damask permission
    let settings = fs::read_to_string(dir.path().join(".claude/settings.json")).unwrap();
    let doc: serde_json::Value = serde_json::from_str(&settings).unwrap();
    let allow = doc["permissions"]["allow"].as_array().unwrap();
    assert!(allow.iter().any(|v| v.as_str() == Some("Bash(damask *)")));
}

#[test]
fn init_claude_merges_into_existing_settings() {
    let dir = TempDir::new().unwrap();

    // First init without --claude
    init_project(&dir);

    // Manually create existing .claude/settings.json with other permissions
    let claude_dir = dir.path().join(".claude");
    fs::create_dir_all(&claude_dir).unwrap();
    let existing = serde_json::json!({
        "permissions": {
            "allow": ["Bash(npm test)", "Read(*)"]
        },
        "model": "opus"
    });
    fs::write(
        claude_dir.join("settings.json"),
        serde_json::to_string_pretty(&existing).unwrap(),
    )
    .unwrap();

    // Run init --claude — should discover existing project and merge
    damask()
        .args(["init", "--claude"])
        .current_dir(dir.path())
        .assert()
        .success()
        .stdout(predicate::str::contains("Found existing .damask/"))
        .stdout(predicate::str::contains("Added \"Bash(damask *)\""));

    // Verify merged: old permissions preserved, new one added
    let settings = fs::read_to_string(claude_dir.join("settings.json")).unwrap();
    let doc: serde_json::Value = serde_json::from_str(&settings).unwrap();
    let allow = doc["permissions"]["allow"].as_array().unwrap();
    assert!(
        allow.iter().any(|v| v.as_str() == Some("Bash(npm test)")),
        "existing permission should be preserved"
    );
    assert!(
        allow.iter().any(|v| v.as_str() == Some("Read(*)")),
        "existing permission should be preserved"
    );
    assert!(
        allow.iter().any(|v| v.as_str() == Some("Bash(damask *)")),
        "damask permission should be added"
    );
    // Non-permission fields preserved
    assert_eq!(doc["model"], "opus", "other settings should be preserved");
}

#[test]
fn init_claude_idempotent_no_duplicate() {
    let dir = TempDir::new().unwrap();

    // First init with --claude
    damask()
        .args(["init", "--claude"])
        .current_dir(dir.path())
        .assert()
        .success();

    // Second run — should discover existing and report already allowlisted
    damask()
        .args(["init", "--claude"])
        .current_dir(dir.path())
        .assert()
        .success()
        .stdout(predicate::str::contains("already allows"));

    // Verify no duplicate entry
    let settings = fs::read_to_string(dir.path().join(".claude/settings.json")).unwrap();
    let doc: serde_json::Value = serde_json::from_str(&settings).unwrap();
    let allow = doc["permissions"]["allow"].as_array().unwrap();
    let damask_count = allow
        .iter()
        .filter(|v| v.as_str() == Some("Bash(damask *)"))
        .count();
    assert_eq!(damask_count, 1, "should have exactly one damask permission entry");
}

#[test]
fn init_claude_installs_hooks() {
    let dir = TempDir::new().unwrap();

    damask()
        .args(["init", "--claude"])
        .current_dir(dir.path())
        .assert()
        .success()
        .stdout(predicate::str::contains("Hooks installed"));

    let settings = fs::read_to_string(dir.path().join(".claude/settings.json")).unwrap();
    let doc: serde_json::Value = serde_json::from_str(&settings).unwrap();

    let hook_command = |doc: &serde_json::Value, event: &str| -> String {
        doc["hooks"][event].as_array().unwrap()[0]["hooks"][0]["command"]
            .as_str()
            .unwrap()
            .to_string()
    };

    // Every hook is guarded: a teammate without damask on PATH must get
    // zero command-not-found errors from the committed settings.json.
    let briefing = hook_command(&doc, "SessionStart");
    assert!(briefing.contains("damask briefing"));
    assert!(
        briefing.contains("command -v damask"),
        "SessionStart hook must be guarded"
    );
    assert!(
        briefing.contains("Install the damask CLI"),
        "SessionStart fallback should advertise how to install"
    );
    let stop = hook_command(&doc, "Stop");
    assert!(stop.contains("damask harvest"));
    assert!(stop.contains("command -v damask"), "Stop hook must be guarded");
    let post_tool_entries = doc["hooks"]["PostToolUse"].as_array().unwrap();
    let post_tool = hook_command(&doc, "PostToolUse");
    assert!(post_tool.contains("damask peek"));
    assert!(
        post_tool.contains("command -v damask"),
        "PostToolUse hook must be guarded"
    );
    assert_eq!(
        post_tool_entries[0]["matcher"], "Read|Edit|Write|MultiEdit|NotebookEdit",
        "peek should only fire on file-touching tools"
    );
    let prompt_submit = hook_command(&doc, "UserPromptSubmit");
    assert!(prompt_submit.contains("damask peek"));
    assert!(prompt_submit.contains("command -v damask"));

    // Second run must not duplicate hook entries.
    damask()
        .args(["init", "--claude"])
        .current_dir(dir.path())
        .assert()
        .success();
    let settings = fs::read_to_string(dir.path().join(".claude/settings.json")).unwrap();
    let doc: serde_json::Value = serde_json::from_str(&settings).unwrap();
    assert_eq!(doc["hooks"]["SessionStart"].as_array().unwrap().len(), 1);
    assert_eq!(doc["hooks"]["Stop"].as_array().unwrap().len(), 1);
    assert_eq!(doc["hooks"]["PostToolUse"].as_array().unwrap().len(), 1);
    assert_eq!(doc["hooks"]["UserPromptSubmit"].as_array().unwrap().len(), 1);
}

#[test]
fn bare_init_in_claude_session_installs_the_loop() {
    // The primary adopter of `damask init` is the agent itself. Inside a
    // live Claude Code session (CLAUDECODE env), bare init must install
    // the full loop — no --claude flag knowledge required — and print an
    // inline warm start plus the commit hint.
    let dir = TempDir::new().unwrap();

    damask()
        .arg("init")
        .env("CLAUDECODE", "1")
        .current_dir(dir.path())
        .assert()
        .success()
        .stdout(predicate::str::contains("Hooks installed"))
        .stdout(predicate::str::contains("Warm start for this session"))
        .stdout(predicate::str::contains("git add .damask .claude"));

    assert!(dir.path().join(".claude/skills/damask/SKILL.md").is_file());
    assert!(dir.path().join(".claude/settings.json").is_file());

    // Without the env (and without .claude/), bare init stays agent-free.
    let dir2 = TempDir::new().unwrap();
    damask()
        .arg("init")
        .current_dir(dir2.path())
        .assert()
        .success()
        .stdout(predicate::str::contains("No AI agent directories detected"));
    assert!(!dir2.path().join(".claude").exists());
}

#[test]
fn bootstrap_seeds_and_is_idempotent() {
    let dir = TempDir::new().unwrap();
    let git = |args: &[&str]| {
        let out = std::process::Command::new("git")
            .args(args)
            .current_dir(dir.path())
            .output()
            .unwrap();
        assert!(out.status.success(), "git {:?} failed", args);
    };
    git(&["init", "-q"]);
    git(&["config", "user.email", "t@t"]);
    git(&["config", "user.name", "t"]);
    fs::write(
        dir.path().join("Cargo.toml"),
        "[package]\nname = \"acme\"\ndescription = \"Test API\"\n",
    )
    .unwrap();
    fs::write(dir.path().join("a.rs"), "// TODO: fix this later\nfn x() {}\n").unwrap();
    fs::write(dir.path().join("b.rs"), "fn y() {}\n").unwrap();
    git(&["add", "-A"]);
    git(&["commit", "-qm", "c1"]);
    // Co-change history: a.rs + b.rs together three more times.
    for i in 0..3 {
        fs::write(dir.path().join("a.rs"), format!("// TODO: fix this later\nfn x() {{}} // {i}\n")).unwrap();
        fs::write(dir.path().join("b.rs"), format!("fn y() {{}} // {i}\n")).unwrap();
        git(&["add", "-A"]);
        git(&["commit", "-qm", "co"]);
    }
    init_project(&dir);

    damask()
        .arg("bootstrap")
        .current_dir(dir.path())
        .assert()
        .success()
        .stdout(predicate::str::contains("manifest describes"))
        .stdout(predicate::str::contains("TODO/FIXME gotchas"))
        .stdout(predicate::str::contains("co-change pairs"));

    let jsonl = fs::read_to_string(dir.path().join(".damask/edges/bootstrap.jsonl")).unwrap();
    assert!(jsonl.contains("\"agent\":\"damask-bootstrap\""), "bootstrap must stamp its agent");
    assert!(jsonl.contains("acme: Test API"), "manifest name/description extracted");
    assert!(jsonl.contains("TODO: fix this later"), "TODO comment becomes gotcha");
    assert!(jsonl.contains("\"status\":\"hypothesis\""), "seeds are hypotheses");
    assert!(jsonl.contains("changed together in"), "co-change pair recorded");

    // Idempotent without --force.
    damask()
        .arg("bootstrap")
        .current_dir(dir.path())
        .assert()
        .success()
        .stdout(predicate::str::contains("Already bootstrapped"));

    // --force regenerates rather than appending duplicates.
    damask()
        .args(["bootstrap", "--force"])
        .current_dir(dir.path())
        .assert()
        .success()
        .stdout(predicate::str::contains("Bootstrapped"));
    let regenerated = fs::read_to_string(dir.path().join(".damask/edges/bootstrap.jsonl")).unwrap();
    let count = |s: &str| s.matches("\"rel\":\"describes\"").count();
    assert_eq!(count(&jsonl), count(&regenerated), "--force must not duplicate facts");

    // Session-1 payoff: peek on the TODO file injects the gotcha.
    damask()
        .args(["peek", "--file", "a.rs", "--session", "s1"])
        .current_dir(dir.path())
        .assert()
        .success()
        .stdout(predicate::str::contains("TODO: fix this later"));
}

#[test]
fn queries_never_dead_end() {
    let dir = TempDir::new().unwrap();
    init_project(&dir);
    set_ns(&dir, "test");
    fs::create_dir_all(dir.path().join("src")).unwrap();
    fs::write(dir.path().join("src/a.rs"), "fn a() {}\nfn b() {}\nfn c() {}\n").unwrap();
    fs::write(dir.path().join("src/b.rs"), "fn d() {}\n").unwrap();

    damask()
        .args(["record", "src/a.rs", "1", "1", "risk", "-m", "read-modify-write race", "-c", "0.8"])
        .current_dir(dir.path())
        .assert()
        .success();
    damask()
        .args(["record", "src/b.rs", "1", "1", "gotcha", "-m", "quirk", "-c", "0.6"])
        .current_dir(dir.path())
        .assert()
        .success();

    // Directory rollup: `at src/` is a heat map, not a dead end.
    damask()
        .args(["at", "src/"])
        .current_dir(dir.path())
        .assert()
        .success()
        .stdout(predicate::str::contains("2 open edges across 2 files"))
        .stdout(predicate::str::contains("src/a.rs"))
        .stdout(predicate::str::contains("Next: damask at"));

    // In-file fallback: a line miss lists the file's annotated regions.
    damask()
        .args(["at", "src/a.rs:3"])
        .current_dir(dir.path())
        .assert()
        .success()
        .stdout(predicate::str::contains("annotated region(s) elsewhere in this file"))
        .stdout(predicate::str::contains("Next: damask at src/a.rs"));

    // FTS syntax characters in payload text are searchable, not a crash.
    damask()
        .args(["search", "read-modify-write"])
        .current_dir(dir.path())
        .assert()
        .success()
        .stdout(predicate::str::contains("read-modify-write race"));

    // Inline AND errors with the corrected multi-arg form.
    damask()
        .args(["where", "rel=risk AND confidence>0.5"])
        .current_dir(dir.path())
        .assert()
        .failure()
        .stderr(predicate::str::contains("\"rel=risk\" \"confidence>0.5\""));

    // Unique id prefix round-trips; unknown full id on follow exits 1.
    let output = damask()
        .args(["--format", "json", "where", "rel=risk"])
        .current_dir(dir.path())
        .output()
        .unwrap();
    let json: serde_json::Value =
        serde_json::from_str(&String::from_utf8(output.stdout).unwrap()).unwrap();
    let full_id = json["edges"][0]["id"].as_str().unwrap();
    damask()
        .args(["endorse", &full_id[..12], r#"{"summary":"confirmed"}"#])
        .current_dir(dir.path())
        .assert()
        .success()
        .stdout(predicate::str::contains(full_id));
    damask()
        .args(["follow", "e_01AAAAAAAAAAAAAAAAAAAAAAAA"])
        .current_dir(dir.path())
        .assert()
        .failure()
        .stderr(predicate::str::contains("no span or edge"));
}

#[test]
fn ruled_out_status_sinks_marks_and_triages() {
    let dir = TempDir::new().unwrap();
    init_project(&dir);
    set_ns(&dir, "test");
    fs::write(dir.path().join("f.rs"), "fn a() {}\nfn b() {}\n").unwrap();

    damask()
        .args(["record", "f.rs", "1", "1", "risk", "-m", "live risk", "-c", "0.6"])
        .current_dir(dir.path())
        .assert()
        .success();
    damask()
        .args(["record", "f.rs", "2", "2", "risk",
               r#"{"summary":"dismissed risk","confidence":0.95,"status":"ruled_out"}"#])
        .current_dir(dir.path())
        .assert()
        .success();

    // Ranking: despite higher confidence, ruled_out sinks below the live risk.
    let out = damask()
        .args(["--format", "json", "where", "rel=risk"])
        .current_dir(dir.path())
        .output()
        .unwrap();
    let json: serde_json::Value =
        serde_json::from_str(&String::from_utf8(out.stdout).unwrap()).unwrap();
    let edges = json["edges"].as_array().unwrap();
    assert_eq!(edges[0]["payload"]["summary"], "live risk");
    assert_eq!(edges[1]["payload"]["status"], "ruled_out");

    // Human output carries the marker.
    damask()
        .args(["where", "rel=risk"])
        .current_dir(dir.path())
        .assert()
        .stdout(predicate::str::contains("[ruled out]"));

    // Triage offers the door out, and takes it on request.
    damask()
        .arg("triage")
        .current_dir(dir.path())
        .assert()
        .stdout(predicate::str::contains("--close-ruled-out"));
    damask()
        .args(["triage", "--close-ruled-out"])
        .current_dir(dir.path())
        .assert()
        .success()
        .stdout(predicate::str::contains("Closed 1 ruled-out edges"));
    let out = damask()
        .args(["where", "rel=risk"])
        .current_dir(dir.path())
        .output()
        .unwrap();
    let stdout = String::from_utf8(out.stdout).unwrap();
    assert!(!stdout.contains("dismissed risk"), "closed ruled-out must vanish");
}

#[test]
fn sweep_reports_and_reanchors_drifted_spans() {
    let dir = TempDir::new().unwrap();
    let git = |args: &[&str]| {
        let out = std::process::Command::new("git")
            .args(args)
            .current_dir(dir.path())
            .output()
            .unwrap();
        assert!(out.status.success(), "git {args:?} failed");
    };
    git(&["init", "-q"]);
    git(&["config", "user.email", "t@t"]);
    git(&["config", "user.name", "t"]);
    fs::write(dir.path().join("f.rs"), "fn keep() {}\nfn also() {}\n").unwrap();
    git(&["add", "-A"]);
    git(&["commit", "-qm", "init"]);
    init_project(&dir);
    set_ns(&dir, "test");

    damask()
        .args(["record", "f.rs", "1", "2", "risk", "-m", "holds", "-c", "0.8"])
        .current_dir(dir.path())
        .assert()
        .success();

    // Drift the anchor: prepend a line and commit.
    fs::write(dir.path().join("f.rs"), "// hdr\nfn keep() {}\nfn also() {}\n").unwrap();
    git(&["add", "-A"]);
    git(&["commit", "-qm", "drift"]);

    damask()
        .arg("sweep")
        .current_dir(dir.path())
        .assert()
        .success()
        .stdout(predicate::str::contains("Drifted (1 spans"))
        .stdout(predicate::str::contains("damask sweep --reanchor"));

    damask()
        .args(["sweep", "--reanchor"])
        .current_dir(dir.path())
        .assert()
        .success()
        .stdout(predicate::str::contains("Re-anchored 1"));

    damask()
        .arg("sweep")
        .current_dir(dir.path())
        .assert()
        .success()
        .stdout(predicate::str::contains("Sweep clean"));

    // The healed anchor follows the drift: content moved to lines 2-3.
    damask()
        .args(["at", "f.rs"])
        .current_dir(dir.path())
        .assert()
        .stdout(predicate::str::contains("f.rs:2-3"))
        .stdout(predicate::str::contains("\u{2705}"));
}

#[test]
fn severity_is_first_class_and_reasons_are_free_text() {
    let dir = TempDir::new().unwrap();
    init_project(&dir);
    set_ns(&dir, "test");
    fs::write(dir.path().join("f.rs"), "a\nb\nc\n").unwrap();

    damask()
        .args(["record", "f.rs", "1", "1", "risk", "-m", "certain but harmless",
               "-c", "0.95", "--severity", "low"])
        .current_dir(dir.path())
        .assert()
        .success();
    damask()
        .args(["record", "f.rs", "2", "2", "risk", "-m", "probable and terrible",
               "-c", "0.7", "--severity", "critical"])
        .current_dir(dir.path())
        .assert()
        .success();

    // Severity orders attention: critical leads despite lower confidence.
    let out = damask()
        .args(["--format", "json", "where", "rel=risk"])
        .current_dir(dir.path())
        .output()
        .unwrap();
    let json: serde_json::Value =
        serde_json::from_str(&String::from_utf8(out.stdout).unwrap()).unwrap();
    assert_eq!(json["edges"][0]["payload"]["severity"], "critical");

    // Filterable and marked.
    damask()
        .args(["where", "severity=critical"])
        .current_dir(dir.path())
        .assert()
        .success()
        .stdout(predicate::str::contains("[critical]"))
        .stdout(predicate::str::contains("probable and terrible"));

    // Invalid severity flag teaches.
    damask()
        .args(["record", "f.rs", "3", "3", "risk", "-m", "x", "--severity", "huge"])
        .current_dir(dir.path())
        .assert()
        .failure();

    // Free-text close reason lands verbatim.
    let eid = json["edges"][1]["id"].as_str().unwrap();
    damask()
        .args(["close", eid, "--reason", "superseded by PR #42"])
        .current_dir(dir.path())
        .assert()
        .success();
    damask()
        .args(["log", "--limit", "0"])
        .current_dir(dir.path())
        .assert()
        .stdout(predicate::str::contains("superseded by PR #42"));
}

#[test]
fn namespace_schemas_validate_rank_and_filter() {
    let dir = TempDir::new().unwrap();
    init_project(&dir);
    fs::write(dir.path().join("contract.md"), "clause 1\nclause 2\nclause 3\n").unwrap();

    // Assert a legal-domain schema on the ns.
    let cfg_path = dir.path().join(".damask/config.json");
    let mut cfg: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&cfg_path).unwrap()).unwrap();
    cfg["namespaces"] = serde_json::json!({
        "legal": {"schema": {"jurisdiction": {
            "enum": ["EU", "US", "UK"], "rank": {"EU": 1.3}}}}
    });
    fs::write(&cfg_path, cfg.to_string()).unwrap();

    let rec = |line: &str, conf: &str, field: &str| {
        damask()
            .args(["--ns", "legal", "record", "contract.md", line, line, "risk",
                   "-m", "finding", "-c", conf, "--field", field])
            .current_dir(dir.path())
            .assert()
    };
    rec("1", "0.8", "jurisdiction=US").success();
    rec("2", "0.6", "jurisdiction=EU").success();

    // record rejects enum violations with a teaching error.
    rec("3", "0.5", "jurisdiction=DE")
        .failure()
        .stderr(predicate::str::contains("must be one of [EU, US, UK]"));

    // batch rejects them too — no write door bypasses the schema.
    let batch = r#"[{"span":{"path":"contract.md","start":3,"end":3}},{"edge":{"from":"$0","to":"_","rel":"risk","payload":{"summary":"x","confidence":0.5,"jurisdiction":"DE"}}}]"#;
    damask()
        .args(["--ns", "legal", "batch", "--stdin"])
        .write_stdin(batch)
        .current_dir(dir.path())
        .assert()
        .failure()
        .stderr(predicate::str::contains("must be one of [EU, US, UK]"));

    // Custom field is filterable; declared rank weight beats confidence.
    let out = damask()
        .args(["--format", "json", "--ns", "legal", "where", "rel=risk"])
        .current_dir(dir.path())
        .output()
        .unwrap();
    let json: serde_json::Value =
        serde_json::from_str(&String::from_utf8(out.stdout).unwrap()).unwrap();
    assert_eq!(json["edges"][0]["payload"]["jurisdiction"], "EU",
        "declared rank weight must outrank higher confidence");
    damask()
        .args(["--ns", "legal", "where", "jurisdiction=EU"])
        .current_dir(dir.path())
        .assert()
        .success()
        .stdout(predicate::str::contains("1-1 of 1"));
}

#[test]
fn log_is_bounded_by_default() {
    let dir = TempDir::new().unwrap();
    init_project(&dir);
    set_ns(&dir, "test");
    fs::write(dir.path().join("f.rs"), "fn x() {}\n").unwrap();
    for i in 0..30 {
        damask()
            .args(["record", "f.rs", "1", "1", "risk", "-m", &format!("finding {i}"), "-c", "0.5"])
            .current_dir(dir.path())
            .assert()
            .success();
    }
    // 30 records = 60 facts; default limit 50 hides the earliest 10.
    damask()
        .arg("log")
        .current_dir(dir.path())
        .assert()
        .success()
        .stdout(predicate::str::contains("10 earlier facts hidden"));
    let output = damask()
        .args(["--format", "json", "log", "--limit", "5"])
        .current_dir(dir.path())
        .output()
        .unwrap();
    let json: serde_json::Value =
        serde_json::from_str(&String::from_utf8(output.stdout).unwrap()).unwrap();
    assert_eq!(json["facts"].as_array().unwrap().len(), 5);
    assert_eq!(json["context"]["showing"]["total"], 60);
}

#[test]
fn dispute_resolution_reason_teaches_close() {
    let dir = TempDir::new().unwrap();
    init_project(&dir);
    set_ns(&dir, "test");
    fs::write(dir.path().join("f.rs"), "fn x() {}\n").unwrap();

    let output = damask()
        .args(["--format", "json", "record", "f.rs", "1", "1", "risk", "-m", "r", "-c", "0.8"])
        .current_dir(dir.path())
        .output()
        .unwrap();
    let facts: serde_json::Value =
        serde_json::from_str(&String::from_utf8(output.stdout).unwrap()).unwrap();
    let edge_id = facts
        .as_array()
        .unwrap()
        .iter()
        .find(|f| f["t"] == "edge")
        .unwrap()["id"]
        .as_str()
        .unwrap()
        .to_string();

    // --reason fixed writes the dispute AND teaches the close.
    damask()
        .args(["dispute", &edge_id, "--reason", "fixed"])
        .current_dir(dir.path())
        .assert()
        .success()
        .stdout(predicate::str::contains("Disputed"))
        .stdout(predicate::str::contains(format!("damask close {edge_id} --reason resolved")));

    // A raw payload starting "Fixed:" gets the same hint.
    damask()
        .args(["dispute", &edge_id, r#"{"summary":"Fixed: in PR #42"}"#])
        .current_dir(dir.path())
        .assert()
        .success()
        .stdout(predicate::str::contains("reads as RESOLVED"));
}

#[test]
fn triage_reports_then_closes_deleted_anchors() {
    let dir = TempDir::new().unwrap();
    let git = |args: &[&str]| {
        let out = std::process::Command::new("git")
            .args(args)
            .current_dir(dir.path())
            .output()
            .unwrap();
        assert!(out.status.success(), "git {args:?} failed");
    };
    git(&["init", "-q"]);
    git(&["config", "user.email", "t@t"]);
    git(&["config", "user.name", "t"]);
    fs::create_dir_all(dir.path().join("src/flow")).unwrap();
    fs::write(dir.path().join("src/flow/gone.rs"), "fn g() {}\n").unwrap();
    fs::write(dir.path().join("live.rs"), "fn l() {}\n").unwrap();
    git(&["add", "-A"]);
    git(&["commit", "-qm", "init"]);
    init_project(&dir);
    set_ns(&dir, "test");

    damask()
        .args(["record", "src/flow/gone.rs", "1", "1", "risk", "-m", "doomed", "-c", "0.8"])
        .current_dir(dir.path())
        .assert()
        .success();
    damask()
        .args(["record", "live.rs", "1", "1", "risk", "-m", "alive", "-c", "0.8"])
        .current_dir(dir.path())
        .assert()
        .success();

    git(&["rm", "-rq", "src/flow"]);
    git(&["commit", "-qm", "remove flow"]);

    // Report proposes, never closes.
    damask()
        .arg("triage")
        .current_dir(dir.path())
        .assert()
        .success()
        .stdout(predicate::str::contains("src/flow/"))
        .stdout(predicate::str::contains("--close-deleted src/flow/"));
    damask()
        .args(["where", "rel=risk"])
        .current_dir(dir.path())
        .assert()
        .stdout(predicate::str::contains("doomed"), );

    // Execute the proposed close: doomed disappears, alive stays.
    damask()
        .args(["triage", "--close-deleted", "src/flow/"])
        .current_dir(dir.path())
        .assert()
        .success()
        .stdout(predicate::str::contains("Closed 1 edges"));
    let out = damask()
        .args(["where", "rel=risk"])
        .current_dir(dir.path())
        .output()
        .unwrap();
    let stdout = String::from_utf8(out.stdout).unwrap();
    assert!(!stdout.contains("doomed"), "closed edge must vanish from where");
    assert!(stdout.contains("alive"), "live edge must remain");
}

#[test]
fn confirm_reanchors_drifted_span() {
    let dir = TempDir::new().unwrap();
    let git = |args: &[&str]| {
        let out = std::process::Command::new("git")
            .args(args)
            .current_dir(dir.path())
            .output()
            .unwrap();
        assert!(out.status.success(), "git {args:?} failed");
    };
    git(&["init", "-q"]);
    git(&["config", "user.email", "t@t"]);
    git(&["config", "user.name", "t"]);
    fs::write(dir.path().join("f.rs"), "fn keep() {}\nfn also() {}\n").unwrap();
    git(&["add", "-A"]);
    git(&["commit", "-qm", "init"]);
    init_project(&dir);
    set_ns(&dir, "test");

    damask()
        .args(["record", "f.rs", "1", "2", "risk", "-m", "still true", "-c", "0.8"])
        .current_dir(dir.path())
        .assert()
        .success();

    // Drift: prepend a line and commit, content moves to 2-3.
    fs::write(dir.path().join("f.rs"), "// header\nfn keep() {}\nfn also() {}\n").unwrap();
    git(&["add", "-A"]);
    git(&["commit", "-qm", "drift"]);

    // at shows the drifted glyph + repair hint with the span id.
    let out = damask()
        .args(["at", "f.rs"])
        .current_dir(dir.path())
        .output()
        .unwrap();
    let stdout = String::from_utf8(out.stdout).unwrap();
    assert!(stdout.contains("f.rs:2-3"), "anchor must track the drift: {stdout}");
    assert!(stdout.contains("damask confirm s_"), "drift hint must name the span");
    let span_id = stdout
        .split("damask confirm ")
        .nth(1)
        .unwrap()
        .split('`')
        .next()
        .unwrap()
        .to_string();

    damask()
        .args(["confirm", &span_id])
        .current_dir(dir.path())
        .assert()
        .success()
        .stdout(predicate::str::contains("Re-anchored"));

    // Healed: ✅, no more drift hint.
    let out = damask()
        .args(["at", "f.rs"])
        .current_dir(dir.path())
        .output()
        .unwrap();
    let stdout = String::from_utf8(out.stdout).unwrap();
    assert!(stdout.contains('\u{2705}'), "confirmed span must show exact: {stdout}");
    assert!(!stdout.contains("drifted —"), "repair hint must clear after confirm");
}

#[test]
fn empty_briefing_advertises_bootstrap() {
    let dir = TempDir::new().unwrap();
    init_project(&dir);
    damask()
        .arg("briefing")
        .current_dir(dir.path())
        .assert()
        .success()
        .stdout(predicate::str::contains("damask bootstrap"));
}

#[test]
fn init_gitignores_active_ns() {
    let dir = TempDir::new().unwrap();
    init_project(&dir);
    let gitignore = fs::read_to_string(dir.path().join(".damask/.gitignore")).unwrap();
    assert!(
        gitignore.lines().any(|l| l == ".active_ns"),
        ".active_ns is per-checkout state and must not be committed"
    );
}

#[test]
fn init_claude_upgrades_unguarded_hooks_in_place() {
    // Older installs wrote bare `damask briefing` etc. Re-running init must
    // upgrade those entries to the guarded form without duplicating them.
    let dir = TempDir::new().unwrap();
    fs::create_dir_all(dir.path().join(".claude")).unwrap();
    fs::write(
        dir.path().join(".claude/settings.json"),
        serde_json::json!({
            "hooks": {
                "SessionStart": [
                    {"matcher": "startup|resume|clear",
                     "hooks": [{"type": "command", "command": "damask briefing"}]}
                ],
                "Stop": [
                    {"hooks": [{"type": "command", "command": "damask harvest"}]}
                ]
            }
        })
        .to_string(),
    )
    .unwrap();

    damask()
        .args(["init", "--claude"])
        .current_dir(dir.path())
        .assert()
        .success()
        .stdout(predicate::str::contains("Updated SessionStart hook"));

    let settings = fs::read_to_string(dir.path().join(".claude/settings.json")).unwrap();
    let doc: serde_json::Value = serde_json::from_str(&settings).unwrap();

    let session_start = doc["hooks"]["SessionStart"].as_array().unwrap();
    assert_eq!(session_start.len(), 1, "upgrade must not duplicate the entry");
    let cmd = session_start[0]["hooks"][0]["command"].as_str().unwrap();
    assert!(cmd.contains("command -v damask") && cmd.contains("damask briefing"));
    assert_eq!(
        session_start[0]["matcher"], "startup|resume|clear",
        "existing matcher must be preserved"
    );

    let stop = doc["hooks"]["Stop"].as_array().unwrap();
    assert_eq!(stop.len(), 1);
    let cmd = stop[0]["hooks"][0]["command"].as_str().unwrap();
    assert!(cmd.contains("command -v damask") && cmd.contains("damask harvest"));
}

#[test]
fn init_writes_union_merge_gitattributes() {
    let dir = TempDir::new().unwrap();
    init_project(&dir);

    let attrs = fs::read_to_string(dir.path().join(".damask/.gitattributes")).unwrap();
    assert!(attrs.contains("edges/*.jsonl merge=union"));
}

#[test]
fn writes_are_stamped_with_ambient_provenance() {
    let dir = TempDir::new().unwrap();
    init_project(&dir);
    set_ns(&dir, "test");

    fs::write(dir.path().join("a.rs"), "fn a() {}\n").unwrap();
    damask()
        .args(["record", "a.rs", "1", "1", "risk", r#"{"summary":"x","confidence":0.9}"#])
        .env("DAMASK_AGENT", "test-agent")
        .env("DAMASK_SESSION", "sess-42")
        .current_dir(dir.path())
        .assert()
        .success();

    let log = fs::read_to_string(dir.path().join(".damask/edges/test.jsonl")).unwrap();
    assert!(log.contains("\"agent\":\"test-agent\""), "agent should be stamped");
    assert!(log.contains("\"session\":\"sess-42\""), "session should be stamped");
}

#[test]
fn claude_code_sessions_stamp_and_count_endorsements_distinctly() {
    // Claude Code exports CLAUDE_CODE_SESSION_ID. Endorsements from two
    // distinct sessions must stamp distinct provenance and count as 2 —
    // not collapse to 1 via the agent-only COALESCE fallback.
    let dir = TempDir::new().unwrap();
    init_project(&dir);
    set_ns(&dir, "test");

    fs::write(dir.path().join("test.rs"), "fn foo() {}\n").unwrap();

    let output = damask()
        .args(["--format", "json", "span", "test.rs", "1", "1"])
        .current_dir(dir.path())
        .output()
        .unwrap();
    let span: serde_json::Value =
        serde_json::from_str(&String::from_utf8(output.stdout).unwrap()).unwrap();
    let span_id = span["id"].as_str().unwrap();

    let output = damask()
        .args([
            "--format",
            "json",
            "edge",
            span_id,
            "_",
            "risk",
            r#"{"summary":"A risk","confidence":0.9}"#,
        ])
        .current_dir(dir.path())
        .output()
        .unwrap();
    let edge: serde_json::Value =
        serde_json::from_str(&String::from_utf8(output.stdout).unwrap()).unwrap();
    let edge_id = edge["id"].as_str().unwrap();

    // Two endorsements from two distinct ambient Claude Code sessions.
    // DAMASK_*/legacy vars are removed so the ambient fallback is what's
    // exercised, hermetically, even when the test itself runs under Claude.
    for sess in ["sess-a", "sess-b"] {
        damask()
            .args(["endorse", edge_id, r#"{"summary":"confirmed"}"#])
            .env_remove("DAMASK_AGENT")
            .env_remove("DAMASK_SESSION")
            .env_remove("CLAUDE_SESSION_ID")
            .env("CLAUDE_CODE_SESSION_ID", sess)
            .current_dir(dir.path())
            .assert()
            .success();
    }

    let jsonl = fs::read_to_string(dir.path().join(".damask/edges/test.jsonl")).unwrap();
    assert!(
        jsonl.contains("\"session\":\"sess-a\"") && jsonl.contains("\"session\":\"sess-b\""),
        "ambient CLAUDE_CODE_SESSION_ID should stamp each endorsement"
    );

    let output = damask()
        .args(["--format", "json", "at", "test.rs"])
        .current_dir(dir.path())
        .output()
        .unwrap();
    let at: serde_json::Value =
        serde_json::from_str(&String::from_utf8(output.stdout).unwrap()).unwrap();
    let edges = at["edges"].as_array().expect("at json has edges");
    let risk = edges
        .iter()
        .find(|e| e["rel"] == "risk")
        .expect("risk edge present");
    assert_eq!(
        risk["endorsements"], 2,
        "two sessions must count as two endorsements, not collapse to one"
    );
}

#[test]
fn peek_file_mode_injects_and_session_dedups() {
    let dir = TempDir::new().unwrap();
    init_project(&dir);
    set_ns(&dir, "test");

    fs::write(dir.path().join("auth.rs"), "fn validate() {}\n").unwrap();
    damask()
        .args([
            "record", "auth.rs", "1", "1", "risk",
            r#"{"summary":"No expiry check","confidence":0.9}"#,
        ])
        .current_dir(dir.path())
        .assert()
        .success();

    // First peek injects.
    damask()
        .args(["peek", "--file", "auth.rs", "--session", "s1"])
        .current_dir(dir.path())
        .assert()
        .success()
        .stdout(predicate::str::contains("No expiry check"));

    // Same session: already seen, silence.
    damask()
        .args(["peek", "--file", "auth.rs", "--session", "s1"])
        .current_dir(dir.path())
        .assert()
        .success()
        .stdout(predicate::str::is_empty());

    // Different session: injected again.
    damask()
        .args(["peek", "--file", "auth.rs", "--session", "s2"])
        .current_dir(dir.path())
        .assert()
        .success()
        .stdout(predicate::str::contains("No expiry check"));
}

#[test]
fn peek_marks_stale_anchors_instead_of_vouching() {
    // An edge whose anchor file no longer exists must be injected with an
    // explicit staleness marker, not presented at bare stored confidence.
    let dir = TempDir::new().unwrap();
    init_project(&dir);
    set_ns(&dir, "test");

    fs::write(dir.path().join("doomed.rs"), "fn gone() {}\n").unwrap();
    damask()
        .args([
            "record", "doomed.rs", "1", "1", "risk",
            r#"{"summary":"Race in gone()","confidence":0.95}"#,
        ])
        .current_dir(dir.path())
        .assert()
        .success();
    fs::remove_file(dir.path().join("doomed.rs")).unwrap();

    damask()
        .args(["peek", "--file", "doomed.rs", "--session", "s1"])
        .current_dir(dir.path())
        .assert()
        .success()
        .stdout(predicate::str::contains("Race in gone()"))
        .stdout(predicate::str::contains("anchor code no longer exists"));
}

#[test]
fn peek_posttooluse_hook_emits_additional_context() {
    let dir = TempDir::new().unwrap();
    init_project(&dir);
    set_ns(&dir, "test");

    fs::write(dir.path().join("auth.rs"), "fn validate() {}\n").unwrap();
    damask()
        .args([
            "record", "auth.rs", "1", "1", "gotcha",
            r#"{"summary":"Validation skipped when cfg missing","confidence":0.85}"#,
        ])
        .current_dir(dir.path())
        .assert()
        .success();

    let root = dir.path().canonicalize().unwrap();
    let hook_input = serde_json::json!({
        "hook_event_name": "PostToolUse",
        "session_id": "s1",
        "tool_name": "Edit",
        "tool_input": {"file_path": root.join("auth.rs").to_string_lossy()},
    })
    .to_string();

    let output = damask()
        .arg("peek")
        .current_dir(dir.path())
        .write_stdin(hook_input)
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let doc: serde_json::Value = serde_json::from_slice(&output).unwrap();
    assert_eq!(doc["hookSpecificOutput"]["hookEventName"], "PostToolUse");
    assert!(doc["hookSpecificOutput"]["additionalContext"]
        .as_str()
        .unwrap()
        .contains("Validation skipped"));
}

#[test]
fn peek_prompt_mode_matches_keywords() {
    let dir = TempDir::new().unwrap();
    init_project(&dir);
    set_ns(&dir, "test");

    fs::write(dir.path().join("auth.rs"), "fn validate() {}\n").unwrap();
    damask()
        .args([
            "record", "auth.rs", "1", "1", "risk",
            r#"{"summary":"Token expiry never validated","confidence":0.9}"#,
        ])
        .current_dir(dir.path())
        .assert()
        .success();

    damask()
        .args(["peek", "--prompt", "please fix the token expiry bug"])
        .current_dir(dir.path())
        .assert()
        .success()
        .stdout(predicate::str::contains("Token expiry never validated"));

    // Unrelated prompt: silence.
    damask()
        .args(["peek", "--prompt", "refactor the frontend carousel widget"])
        .current_dir(dir.path())
        .assert()
        .success()
        .stdout(predicate::str::is_empty());
}

#[test]
fn peek_ignores_non_file_tools_and_fails_open() {
    let dir = TempDir::new().unwrap();
    init_project(&dir);

    let hook_input = serde_json::json!({
        "hook_event_name": "PostToolUse",
        "session_id": "s1",
        "tool_name": "Bash",
        "tool_input": {"command": "ls"},
    })
    .to_string();
    damask()
        .arg("peek")
        .current_dir(dir.path())
        .write_stdin(hook_input)
        .assert()
        .success()
        .stdout(predicate::str::is_empty());

    damask()
        .arg("peek")
        .current_dir(dir.path())
        .write_stdin("garbage")
        .assert()
        .success()
        .stdout(predicate::str::is_empty());
}

#[test]
fn verify_runs_checks_and_auto_records_once() {
    let dir = TempDir::new().unwrap();
    init_project(&dir);
    set_ns(&dir, "test");

    fs::write(dir.path().join("a.rs"), "fn a() {}\n").unwrap();
    damask()
        .args([
            "record", "a.rs", "1", "1", "risk",
            r#"{"summary":"holds","confidence":0.9,"check":"true"}"#,
        ])
        .current_dir(dir.path())
        .assert()
        .success();
    damask()
        .args([
            "record", "a.rs", "1", "1", "risk",
            r#"{"summary":"broken","confidence":0.9,"check":"false"}"#,
        ])
        .current_dir(dir.path())
        .assert()
        .success();

    // Plain verify: reports both, exits non-zero because one fails.
    damask()
        .arg("verify")
        .current_dir(dir.path())
        .assert()
        .failure()
        .stdout(predicate::str::contains("1/2 checks passed"));

    // --auto: endorses the pass, disputes the failure.
    damask()
        .args(["verify", "--auto"])
        .current_dir(dir.path())
        .assert()
        .failure()
        .stdout(predicate::str::contains("endorsed"))
        .stdout(predicate::str::contains("disputed"));

    // Idempotent: outcomes recorded at most once per kind.
    damask()
        .args(["verify", "--auto"])
        .current_dir(dir.path())
        .assert()
        .failure();
    let log = fs::read_to_string(dir.path().join(".damask/edges/test.jsonl")).unwrap();
    let auto_count = log.matches("\"check_auto\":true").count();
    assert_eq!(auto_count, 2, "one auto-endorsement and one auto-dispute, no duplicates");
}

#[test]
fn harvest_quality_nudge_on_deficient_session_edges() {
    let dir = TempDir::new().unwrap();
    init_project(&dir);
    set_ns(&dir, "test");

    // A session-new edge with fields but no summary → lint error.
    fs::write(dir.path().join("a.rs"), "fn a() {}\n").unwrap();
    damask()
        .args(["record", "a.rs", "1", "1", "risk", r#"{"confidence":0.4}"#])
        .current_dir(dir.path())
        .assert()
        .success();

    let root = dir.path().canonicalize().unwrap();
    let transcript = root.join("transcript.jsonl");
    let line = serde_json::json!({
        "type": "assistant",
        "timestamp": "2020-01-01T00:00:00.000Z",
        "message": {"role": "assistant", "content": [
            {"type": "tool_use", "name": "Bash",
             "input": {"command": "damask record a.rs 1 1 risk '{}'"}}
        ]}
    });
    fs::write(&transcript, format!("{line}\n")).unwrap();

    let output = damask()
        .args(["harvest", "--transcript", &transcript.to_string_lossy()])
        .current_dir(dir.path())
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let doc: serde_json::Value = serde_json::from_slice(&output).unwrap();
    assert_eq!(doc["decision"], "block");
    assert!(doc["reason"]
        .as_str()
        .unwrap()
        .contains("quality problems"));
}

#[test]
fn briefing_surfaces_suspect_spans() {
    let dir = TempDir::new().unwrap();
    init_project(&dir);
    set_ns(&dir, "test");

    fs::write(dir.path().join("a.rs"), "fn original() {}\nfn other() {}\n").unwrap();
    damask()
        .args([
            "record", "a.rs", "1", "1", "risk",
            r#"{"summary":"original is risky","confidence":0.9}"#,
        ])
        .current_dir(dir.path())
        .assert()
        .success();

    // Rewrite the annotated line so the content hash no longer matches.
    fs::write(dir.path().join("a.rs"), "fn rewritten_completely() {}\nfn other() {}\n").unwrap();

    damask()
        .arg("briefing")
        .current_dir(dir.path())
        .assert()
        .success()
        .stdout(predicate::str::contains("Suspect annotations"));
}

#[test]
fn search_where_filters_and_ranks() {
    let dir = TempDir::new().unwrap();
    init_project(&dir);
    set_ns(&dir, "test");

    fs::write(dir.path().join("a.rs"), "fn a() {}\n").unwrap();
    damask()
        .args([
            "record", "a.rs", "1", "1", "risk",
            r#"{"summary":"timeout risk high confidence","confidence":0.95}"#,
        ])
        .current_dir(dir.path())
        .assert()
        .success();
    damask()
        .args([
            "record", "a.rs", "1", "1", "risk",
            r#"{"summary":"timeout risk low confidence","confidence":0.3}"#,
        ])
        .current_dir(dir.path())
        .assert()
        .success();

    damask()
        .args(["search", "timeout", "--where", "confidence>=0.9"])
        .current_dir(dir.path())
        .assert()
        .success()
        .stdout(predicate::str::contains("high confidence"))
        .stdout(predicate::str::contains("low confidence").not());

    // Without the filter, ranking puts high confidence first.
    let output = damask()
        .args(["search", "timeout"])
        .current_dir(dir.path())
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let text = String::from_utf8(output).unwrap();
    let high_pos = text.find("high confidence").unwrap();
    let low_pos = text.find("low confidence").unwrap();
    assert!(high_pos < low_pos, "higher-quality edge should rank first");
}

#[test]
fn enrich_annotates_piped_results() {
    let dir = TempDir::new().unwrap();
    init_project(&dir);
    set_ns(&dir, "test");

    fs::write(dir.path().join("auth.rs"), "fn validate() {}\nfn other() {}\n").unwrap();
    damask()
        .args([
            "record", "auth.rs", "1", "1", "risk",
            r#"{"summary":"expiry unchecked","confidence":0.9}"#,
        ])
        .current_dir(dir.path())
        .assert()
        .success();

    let root = dir.path().canonicalize().unwrap();
    // ck --jsonl shape, overlapping the annotated line.
    let input = serde_json::json!({
        "path": root.join("auth.rs").to_string_lossy(),
        "span": {"byte_start": 0, "byte_end": 10, "line_start": 1, "line_end": 2},
        "score": 0.8
    })
    .to_string();

    damask()
        .arg("enrich")
        .current_dir(dir.path())
        .write_stdin(input.clone())
        .assert()
        .success()
        .stdout(predicate::str::contains("expiry unchecked"));

    // JSON mode: augmented passthrough with a "damask" key; junk lines pass through.
    let output = damask()
        .args(["enrich", "--format", "json"])
        .current_dir(dir.path())
        .write_stdin(format!("{input}\nnot json\n"))
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let text = String::from_utf8(output).unwrap();
    let mut lines = text.lines();
    let augmented: serde_json::Value = serde_json::from_str(lines.next().unwrap()).unwrap();
    assert_eq!(augmented["score"], 0.8, "original fields preserved");
    assert!(
        augmented["damask"]["edges"][0]["payload"]["summary"]
            .as_str()
            .unwrap()
            .contains("expiry"),
        "damask annotations attached"
    );
    assert_eq!(lines.next().unwrap(), "not json", "junk passes through untouched");
}

#[test]
fn enrich_results_outside_annotated_range_get_nothing() {
    let dir = TempDir::new().unwrap();
    init_project(&dir);
    set_ns(&dir, "test");

    fs::write(dir.path().join("auth.rs"), "fn a() {}\nfn b() {}\nfn c() {}\n").unwrap();
    damask()
        .args(["record", "auth.rs", "1", "1", "risk", r#"{"summary":"x","confidence":0.9}"#])
        .current_dir(dir.path())
        .assert()
        .success();

    let root = dir.path().canonicalize().unwrap();
    let input = serde_json::json!({
        "path": root.join("auth.rs").to_string_lossy(),
        "span": {"line_start": 3, "line_end": 3}
    })
    .to_string();

    damask()
        .arg("enrich")
        .current_dir(dir.path())
        .write_stdin(input)
        .assert()
        .success()
        .stdout(predicate::str::contains("no damask annotations"));
}

#[test]
fn search_sem_falls_back_without_ck() {
    let dir = TempDir::new().unwrap();
    init_project(&dir);
    set_ns(&dir, "test");

    fs::write(dir.path().join("a.rs"), "fn a() {}\n").unwrap();
    damask()
        .args(["record", "a.rs", "1", "1", "risk", r#"{"summary":"timeout bug","confidence":0.9}"#])
        .current_dir(dir.path())
        .assert()
        .success();

    // Restrict PATH so ck (in ~/.cargo/bin) is invisible: --sem must fall
    // back to keyword search with a hint, not fail.
    damask()
        .args(["search", "timeout", "--sem"])
        .env("PATH", "/usr/bin:/bin")
        .current_dir(dir.path())
        .assert()
        .success()
        .stdout(predicate::str::contains("timeout bug"))
        .stdout(predicate::str::contains("keyword"))
        .stderr(predicate::str::contains("ck"));
}

/// Gated end-to-end semantic test: run with DAMASK_TEST_CK=1 when ck is
/// installed and its embedding model is cached.
#[test]
fn search_sem_uses_ck_when_available() {
    if std::env::var("DAMASK_TEST_CK").as_deref() != Ok("1") {
        eprintln!("skipped (set DAMASK_TEST_CK=1 to run)");
        return;
    }
    let dir = TempDir::new().unwrap();
    init_project(&dir);
    set_ns(&dir, "test");

    fs::write(dir.path().join("a.rs"), "fn a() {}\n").unwrap();
    damask()
        .args([
            "record", "a.rs", "1", "1", "risk",
            r#"{"summary":"credentials are not validated before use","confidence":0.9}"#,
        ])
        .current_dir(dir.path())
        .assert()
        .success();

    damask()
        .args(["search", "authentication checks missing", "--sem"])
        .current_dir(dir.path())
        .assert()
        .success()
        .stdout(predicate::str::contains("semantic"))
        .stdout(predicate::str::contains("credentials"));
}

#[test]
fn batch_bad_endpoint_error_teaches_syntax() {
    let dir = TempDir::new().unwrap();
    init_project(&dir);
    set_ns(&dir, "test");

    fs::write(dir.path().join("a.rs"), "fn a() {}\n").unwrap();
    let batch = serde_json::json!([
        {"span": {"path": "a.rs", "start": 1, "end": 1}},
        {"edge": {"from": "record", "to": "findings", "rel": "depends_on",
                  "payload": {"summary": "x"}}}
    ])
    .to_string();

    let output = damask()
        .args(["batch", "--stdin"])
        .current_dir(dir.path())
        .write_stdin(batch)
        .assert()
        .failure()
        .get_output()
        .stderr
        .clone();
    let err = String::from_utf8(output).unwrap();
    assert!(err.contains("\"record\" is not a valid endpoint"), "names the bad value: {err}");
    assert!(err.contains("$N"), "teaches back-reference syntax");
    assert!(err.contains("help batch"), "points to the reference");
}

#[test]
fn init_skill_sync_is_idempotent_and_self_healing() {
    let dir = TempDir::new().unwrap();

    damask()
        .args(["init", "--claude"])
        .current_dir(dir.path())
        .assert()
        .success()
        .stdout(predicate::str::contains("Created .claude/skills/damask/SKILL.md"));

    // Unchanged: no rewrite.
    damask()
        .args(["init", "--claude"])
        .current_dir(dir.path())
        .assert()
        .success()
        .stdout(predicate::str::contains("already current"));

    // Stale/corrupted copy: refreshed.
    let skill_path = dir.path().join(".claude/skills/damask/SKILL.md");
    fs::write(&skill_path, "# Damask\ncorrupted old copy\n").unwrap();
    damask()
        .args(["init", "--claude"])
        .current_dir(dir.path())
        .assert()
        .success()
        .stdout(predicate::str::contains("Updated .claude/skills/damask/SKILL.md"));
    let restored = fs::read_to_string(&skill_path).unwrap();
    assert!(restored.contains("## Workflow"), "canonical content restored");
}

#[test]
fn briefing_warns_when_installed_skill_is_stale() {
    let dir = TempDir::new().unwrap();

    damask()
        .args(["init", "--claude"])
        .current_dir(dir.path())
        .assert()
        .success();

    fs::write(
        dir.path().join(".claude/skills/damask/SKILL.md"),
        "# Damask\nstale\n",
    )
    .unwrap();

    damask()
        .arg("briefing")
        .current_dir(dir.path())
        .assert()
        .success()
        .stdout(predicate::str::contains("out of date"))
        .stdout(predicate::str::contains("damask init --claude"));
}

#[test]
fn review_markdown_is_pr_ready() {
    let dir = TempDir::new().unwrap();
    init_project(&dir);
    set_ns(&dir, "test");

    fs::write(dir.path().join("a.rs"), "fn a() {}\n").unwrap();
    damask()
        .args([
            "record", "a.rs", "1", "1", "risk",
            r#"{"summary":"needs review","confidence":0.9,"action":"check it"}"#,
        ])
        .current_dir(dir.path())
        .assert()
        .success();

    damask()
        .args(["review", "--markdown"])
        .current_dir(dir.path())
        .assert()
        .success()
        .stdout(predicate::str::contains("## Damask review"))
        .stdout(predicate::str::contains("**risk**"))
        .stdout(predicate::str::contains("needs review"))
        .stdout(predicate::str::contains("action: check it"));
}

#[test]
fn briefing_outside_project_is_silent() {
    let dir = TempDir::new().unwrap();

    damask()
        .arg("briefing")
        .current_dir(dir.path())
        .assert()
        .success()
        .stdout(predicate::str::is_empty());
}

#[test]
fn briefing_cold_start_message() {
    let dir = TempDir::new().unwrap();
    init_project(&dir);

    damask()
        .arg("briefing")
        .current_dir(dir.path())
        .assert()
        .success()
        .stdout(predicate::str::contains("cold start"))
        .stdout(predicate::str::contains("damask help cold-start"));
}

#[test]
fn briefing_warm_shows_findings() {
    let dir = TempDir::new().unwrap();
    init_project(&dir);
    set_ns(&dir, "test");

    fs::write(dir.path().join("auth.rs"), "fn validate() {}\n").unwrap();
    damask()
        .args([
            "record",
            "auth.rs",
            "1",
            "1",
            "risk",
            r#"{"summary":"No token expiry check","confidence":0.9}"#,
        ])
        .current_dir(dir.path())
        .assert()
        .success();

    damask()
        .arg("briefing")
        .current_dir(dir.path())
        .assert()
        .success()
        .stdout(predicate::str::contains("Damask knowledge graph"))
        .stdout(predicate::str::contains("### risk (1)"))
        .stdout(predicate::str::contains("No token expiry check"))
        .stdout(predicate::str::contains("damask at <file>[:line]"));
}

#[test]
fn briefing_json_wraps_session_start_hook_output() {
    let dir = TempDir::new().unwrap();
    init_project(&dir);

    let output = damask()
        .args(["briefing", "--format", "json"])
        .current_dir(dir.path())
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let doc: serde_json::Value = serde_json::from_slice(&output).unwrap();
    assert_eq!(doc["hookSpecificOutput"]["hookEventName"], "SessionStart");
    assert!(doc["hookSpecificOutput"]["additionalContext"]
        .as_str()
        .unwrap()
        .contains("cold start"));
}

/// Build a minimal Claude Code transcript line for a tool_use.
fn transcript_tool_use(name: &str, input: serde_json::Value) -> String {
    serde_json::json!({
        "type": "assistant",
        "message": {
            "role": "assistant",
            "content": [{"type": "tool_use", "name": name, "input": input}]
        }
    })
    .to_string()
}

#[test]
fn harvest_blocks_when_edits_unrecorded() {
    let dir = TempDir::new().unwrap();
    init_project(&dir);

    // Transcript paths must match the project root as the CLI resolves it
    // (macOS tempdirs are symlinked), so canonicalize.
    let root = dir.path().canonicalize().unwrap();
    fs::write(root.join("auth.rs"), "fn validate() {}\n").unwrap();
    let transcript = root.join("transcript.jsonl");
    fs::write(
        &transcript,
        transcript_tool_use(
            "Edit",
            serde_json::json!({"file_path": root.join("auth.rs").to_string_lossy()}),
        ) + "\n",
    )
    .unwrap();

    let output = damask()
        .args(["harvest", "--transcript", &transcript.to_string_lossy()])
        .current_dir(dir.path())
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let doc: serde_json::Value = serde_json::from_slice(&output).unwrap();
    assert_eq!(doc["decision"], "block");
    let reason = doc["reason"].as_str().unwrap();
    assert!(reason.contains("auth.rs"), "reason should list edited file");
    assert!(reason.contains("damask record"), "reason should show how to record");
}

#[test]
fn harvest_allows_when_findings_recorded() {
    let dir = TempDir::new().unwrap();
    init_project(&dir);

    let root = dir.path().canonicalize().unwrap();
    let transcript = root.join("transcript.jsonl");
    fs::write(
        &transcript,
        [
            transcript_tool_use(
                "Edit",
                serde_json::json!({"file_path": root.join("auth.rs").to_string_lossy()}),
            ),
            transcript_tool_use(
                "Bash",
                serde_json::json!({"command": "damask record auth.rs 1 1 risk '{}'"}),
            ),
        ]
        .join("\n"),
    )
    .unwrap();

    damask()
        .args(["harvest", "--transcript", &transcript.to_string_lossy()])
        .current_dir(dir.path())
        .assert()
        .success()
        .stdout(predicate::str::is_empty());
}

#[test]
fn harvest_allows_readonly_sessions() {
    let dir = TempDir::new().unwrap();
    init_project(&dir);

    let root = dir.path().canonicalize().unwrap();
    let transcript = root.join("transcript.jsonl");
    fs::write(
        &transcript,
        transcript_tool_use("Bash", serde_json::json!({"command": "cargo test"})) + "\n",
    )
    .unwrap();

    damask()
        .args(["harvest", "--transcript", &transcript.to_string_lossy()])
        .current_dir(dir.path())
        .assert()
        .success()
        .stdout(predicate::str::is_empty());
}

#[test]
fn harvest_reads_hook_json_from_stdin() {
    let dir = TempDir::new().unwrap();
    init_project(&dir);

    let root = dir.path().canonicalize().unwrap();
    fs::write(root.join("auth.rs"), "fn validate() {}\n").unwrap();
    let transcript = root.join("transcript.jsonl");
    fs::write(
        &transcript,
        transcript_tool_use(
            "Edit",
            serde_json::json!({"file_path": root.join("auth.rs").to_string_lossy()}),
        ) + "\n",
    )
    .unwrap();

    let hook_input = serde_json::json!({
        "session_id": "test",
        "transcript_path": transcript.to_string_lossy(),
        "stop_hook_active": false,
    })
    .to_string();

    damask()
        .arg("harvest")
        .current_dir(dir.path())
        .write_stdin(hook_input)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"decision\":\"block\""));
}

#[test]
fn harvest_never_blocks_twice() {
    let dir = TempDir::new().unwrap();
    init_project(&dir);

    let root = dir.path().canonicalize().unwrap();
    let transcript = root.join("transcript.jsonl");
    fs::write(
        &transcript,
        transcript_tool_use(
            "Edit",
            serde_json::json!({"file_path": root.join("auth.rs").to_string_lossy()}),
        ) + "\n",
    )
    .unwrap();

    let hook_input = serde_json::json!({
        "session_id": "test",
        "transcript_path": transcript.to_string_lossy(),
        "stop_hook_active": true,
    })
    .to_string();

    damask()
        .arg("harvest")
        .current_dir(dir.path())
        .write_stdin(hook_input)
        .assert()
        .success()
        .stdout(predicate::str::is_empty());
}

#[test]
fn harvest_outside_project_is_silent() {
    let dir = TempDir::new().unwrap();

    damask()
        .arg("harvest")
        .current_dir(dir.path())
        .write_stdin("not even json")
        .assert()
        .success()
        .stdout(predicate::str::is_empty());
}

#[test]
fn help_shows_all_commands() {
    damask()
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("init"))
        .stdout(predicate::str::contains("span"))
        .stdout(predicate::str::contains("edge"))
        .stdout(predicate::str::contains("record"))
        .stdout(predicate::str::contains("batch"))
        .stdout(predicate::str::contains("at"))
        .stdout(predicate::str::contains("follow"))
        .stdout(predicate::str::contains("endorse"))
        .stdout(predicate::str::contains("dispute"))
        .stdout(predicate::str::contains("tui"));
}

// ── record command tests ──────────────────────────────────────────

#[test]
fn record_creates_span_and_edge() {
    let dir = TempDir::new().unwrap();
    init_project(&dir);
    set_ns(&dir, "test");

    fs::write(dir.path().join("test.rs"), "fn foo() {}\nfn bar() {}\n").unwrap();

    let output = damask()
        .args([
            "record",
            "test.rs",
            "1",
            "2",
            "risk",
            r#"{"summary":"A risk","confidence":0.9}"#,
        ])
        .current_dir(dir.path())
        .output()
        .unwrap();

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("s_"), "should print span ID");
    assert!(stdout.contains("e_"), "should print edge ID");
    assert!(stdout.contains("[risk]"), "should show rel type");

    // Verify JSONL has exactly 2 lines
    let jsonl = fs::read_to_string(dir.path().join(".damask/edges/test.jsonl")).unwrap();
    let lines: Vec<&str> = jsonl.trim().lines().collect();
    assert_eq!(lines.len(), 2);
    assert!(lines[0].contains("\"t\":\"span\""));
    assert!(lines[1].contains("\"t\":\"edge\""));

    // Verify edge.from == span.id
    let span: serde_json::Value = serde_json::from_str(lines[0]).unwrap();
    let edge: serde_json::Value = serde_json::from_str(lines[1]).unwrap();
    assert_eq!(edge["from"], span["id"]);
    assert!(edge["to"].is_null());
}

#[test]
fn record_json_output_is_array() {
    let dir = TempDir::new().unwrap();
    init_project(&dir);
    set_ns(&dir, "test");

    fs::write(dir.path().join("test.rs"), "fn foo() {}\n").unwrap();

    let output = damask()
        .args([
            "--format",
            "json",
            "record",
            "test.rs",
            "1",
            "1",
            "risk",
            r#"{"summary":"test","confidence":0.9}"#,
        ])
        .current_dir(dir.path())
        .output()
        .unwrap();

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    let facts: Vec<serde_json::Value> = serde_json::from_str(&stdout).unwrap();
    assert_eq!(facts.len(), 2);
    assert_eq!(facts[0]["t"], "span");
    assert_eq!(facts[1]["t"], "edge");
    assert_eq!(facts[1]["from"], facts[0]["id"]);
}

#[test]
fn record_with_symbol() {
    let dir = TempDir::new().unwrap();
    init_project(&dir);
    set_ns(&dir, "test");

    fs::write(dir.path().join("test.rs"), "fn foo() {}\n").unwrap();

    let output = damask()
        .args([
            "--format",
            "json",
            "record",
            "test.rs",
            "1",
            "1",
            "risk",
            r#"{"summary":"test"}"#,
            "--symbol",
            "foo",
        ])
        .current_dir(dir.path())
        .output()
        .unwrap();

    assert!(output.status.success());
    let facts: Vec<serde_json::Value> =
        serde_json::from_str(&String::from_utf8(output.stdout).unwrap()).unwrap();
    assert_eq!(facts[0]["symbol"], "foo");
}

#[test]
fn record_with_to_endpoint() {
    let dir = TempDir::new().unwrap();
    init_project(&dir);
    set_ns(&dir, "test");

    fs::write(dir.path().join("a.rs"), "fn a() {}\n").unwrap();
    fs::write(dir.path().join("b.rs"), "fn b() {}\n").unwrap();

    // Create a target span first
    let output = damask()
        .args(["--format", "json", "span", "b.rs", "1", "1"])
        .current_dir(dir.path())
        .output()
        .unwrap();
    let target: serde_json::Value =
        serde_json::from_str(&String::from_utf8(output.stdout).unwrap()).unwrap();
    let target_id = target["id"].as_str().unwrap();

    // Record with --to pointing to the target
    let output = damask()
        .args([
            "--format",
            "json",
            "record",
            "a.rs",
            "1",
            "1",
            "depends_on",
            r#"{"summary":"a depends on b"}"#,
            "--to",
            target_id,
        ])
        .current_dir(dir.path())
        .output()
        .unwrap();

    assert!(output.status.success());
    let facts: Vec<serde_json::Value> =
        serde_json::from_str(&String::from_utf8(output.stdout).unwrap()).unwrap();
    assert_eq!(facts[1]["to"], target_id);
}

#[test]
fn record_rejects_missing_file() {
    let dir = TempDir::new().unwrap();
    init_project(&dir);
    set_ns(&dir, "test");

    damask()
        .args([
            "record",
            "nonexistent.rs",
            "1",
            "1",
            "risk",
            r#"{"summary":"test"}"#,
        ])
        .current_dir(dir.path())
        .assert()
        .failure()
        .stderr(predicate::str::contains("file not found"));

    // Nothing should have been written
    assert!(!dir.path().join(".damask/edges/test.jsonl").exists());
}

#[test]
fn record_rejects_invalid_json() {
    let dir = TempDir::new().unwrap();
    init_project(&dir);
    set_ns(&dir, "test");

    fs::write(dir.path().join("test.rs"), "x\n").unwrap();

    damask()
        .args(["record", "test.rs", "1", "1", "risk", "not json"])
        .current_dir(dir.path())
        .assert()
        .failure()
        .stderr(predicate::str::contains("not valid JSON"));
}

#[test]
fn record_rejects_invalid_range() {
    let dir = TempDir::new().unwrap();
    init_project(&dir);
    set_ns(&dir, "test");

    fs::write(dir.path().join("test.rs"), "x\n").unwrap();

    damask()
        .args([
            "record",
            "test.rs",
            "5",
            "1",
            "risk",
            r#"{"summary":"test"}"#,
        ])
        .current_dir(dir.path())
        .assert()
        .failure()
        .stderr(predicate::str::contains("start line"));
}

#[test]
fn record_respects_ns_override() {
    let dir = TempDir::new().unwrap();
    init_project(&dir);

    fs::write(dir.path().join("test.rs"), "x\n").unwrap();

    damask()
        .args([
            "--ns",
            "custom-ns",
            "record",
            "test.rs",
            "1",
            "1",
            "risk",
            r#"{"summary":"test"}"#,
        ])
        .current_dir(dir.path())
        .assert()
        .success();

    assert!(dir.path().join(".damask/edges/custom-ns.jsonl").exists());
}

// ── batch command tests ───────────────────────────────────────────

#[test]
fn batch_creates_multiple_facts() {
    let dir = TempDir::new().unwrap();
    init_project(&dir);
    set_ns(&dir, "test");

    fs::write(dir.path().join("a.rs"), "fn a() {}\n").unwrap();
    fs::write(dir.path().join("b.rs"), "fn b() {}\n").unwrap();

    let batch = r#"[
        {"span": {"path":"a.rs", "start":1, "end":1}},
        {"edge": {"from":"$0", "to":"_", "rel":"risk", "payload":{"summary":"risk on a","confidence":0.9}}},
        {"span": {"path":"b.rs", "start":1, "end":1}},
        {"edge": {"from":"$0", "to":"$2", "rel":"depends_on", "payload":{"summary":"a depends on b"}}}
    ]"#;

    let batch_file = dir.path().join("batch.json");
    fs::write(&batch_file, batch).unwrap();

    let output = damask()
        .args(["batch", "-f", "batch.json"])
        .current_dir(dir.path())
        .output()
        .unwrap();

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("4 facts created"));

    // Verify JSONL has 4 lines
    let jsonl = fs::read_to_string(dir.path().join(".damask/edges/test.jsonl")).unwrap();
    let lines: Vec<&str> = jsonl.trim().lines().collect();
    assert_eq!(lines.len(), 4);
}

#[test]
fn batch_backref_resolves() {
    let dir = TempDir::new().unwrap();
    init_project(&dir);
    set_ns(&dir, "test");

    fs::write(dir.path().join("test.rs"), "fn foo() {}\n").unwrap();

    let batch = r#"[
        {"span": {"path":"test.rs", "start":1, "end":1}},
        {"edge": {"from":"$0", "to":"_", "rel":"risk", "payload":{"summary":"test"}}}
    ]"#;

    let batch_file = dir.path().join("batch.json");
    fs::write(&batch_file, batch).unwrap();

    let output = damask()
        .args(["--format", "json", "batch", "-f", "batch.json"])
        .current_dir(dir.path())
        .output()
        .unwrap();

    assert!(output.status.success());
    let facts: Vec<serde_json::Value> =
        serde_json::from_str(&String::from_utf8(output.stdout).unwrap()).unwrap();
    assert_eq!(facts.len(), 2);
    assert_eq!(facts[0]["t"], "span");
    assert_eq!(facts[1]["t"], "edge");
    // edge.from should equal span.id
    assert_eq!(facts[1]["from"], facts[0]["id"]);
}

#[test]
fn batch_cross_span_reference() {
    let dir = TempDir::new().unwrap();
    init_project(&dir);
    set_ns(&dir, "test");

    fs::write(dir.path().join("a.rs"), "fn a() {}\n").unwrap();
    fs::write(dir.path().join("b.rs"), "fn b() {}\n").unwrap();

    let batch = r#"[
        {"span": {"path":"a.rs", "start":1, "end":1}},
        {"edge": {"from":"$0", "to":"_", "rel":"risk", "payload":{"summary":"risk"}}},
        {"span": {"path":"b.rs", "start":1, "end":1}},
        {"edge": {"from":"$0", "to":"$2", "rel":"depends_on", "payload":{"summary":"dep"}}}
    ]"#;

    let batch_file = dir.path().join("batch.json");
    fs::write(&batch_file, batch).unwrap();

    let output = damask()
        .args(["--format", "json", "batch", "-f", "batch.json"])
        .current_dir(dir.path())
        .output()
        .unwrap();

    assert!(output.status.success());
    let facts: Vec<serde_json::Value> =
        serde_json::from_str(&String::from_utf8(output.stdout).unwrap()).unwrap();
    // facts[3] edge should have from=$0 (span a) and to=$2 (span b)
    assert_eq!(facts[3]["from"], facts[0]["id"]);
    assert_eq!(facts[3]["to"], facts[2]["id"]);
}

#[test]
fn batch_rejects_forward_reference() {
    let dir = TempDir::new().unwrap();
    init_project(&dir);
    set_ns(&dir, "test");

    fs::write(dir.path().join("test.rs"), "x\n").unwrap();

    let batch = r#"[
        {"edge": {"from":"$1", "to":"_", "rel":"risk", "payload":{"summary":"test"}}},
        {"span": {"path":"test.rs", "start":1, "end":1}}
    ]"#;

    let batch_file = dir.path().join("batch.json");
    fs::write(&batch_file, batch).unwrap();

    damask()
        .args(["batch", "-f", "batch.json"])
        .current_dir(dir.path())
        .assert()
        .failure()
        .stderr(predicate::str::contains("must refer to an earlier item"));
}

#[test]
fn batch_rejects_self_reference() {
    let dir = TempDir::new().unwrap();
    init_project(&dir);
    set_ns(&dir, "test");

    fs::write(dir.path().join("test.rs"), "x\n").unwrap();

    let batch = r#"[
        {"span": {"path":"test.rs", "start":1, "end":1}},
        {"edge": {"from":"$1", "to":"_", "rel":"risk", "payload":{"summary":"test"}}}
    ]"#;

    let batch_file = dir.path().join("batch.json");
    fs::write(&batch_file, batch).unwrap();

    damask()
        .args(["batch", "-f", "batch.json"])
        .current_dir(dir.path())
        .assert()
        .failure()
        .stderr(predicate::str::contains("must refer to an earlier item"));
}

#[test]
fn batch_rejects_missing_file() {
    let dir = TempDir::new().unwrap();
    init_project(&dir);
    set_ns(&dir, "test");

    let batch = r#"[
        {"span": {"path":"nonexistent.rs", "start":1, "end":1}}
    ]"#;

    let batch_file = dir.path().join("batch.json");
    fs::write(&batch_file, batch).unwrap();

    damask()
        .args(["batch", "-f", "batch.json"])
        .current_dir(dir.path())
        .assert()
        .failure()
        .stderr(predicate::str::contains("file not found"));

    // Nothing should have been written
    assert!(!dir.path().join(".damask/edges/test.jsonl").exists());
}

#[test]
fn batch_rejects_empty_array() {
    let dir = TempDir::new().unwrap();
    init_project(&dir);
    set_ns(&dir, "test");

    let batch_file = dir.path().join("batch.json");
    fs::write(&batch_file, "[]").unwrap();

    damask()
        .args(["batch", "-f", "batch.json"])
        .current_dir(dir.path())
        .assert()
        .failure()
        .stderr(predicate::str::contains("batch is empty"));
}

#[test]
fn batch_json_output_is_array() {
    let dir = TempDir::new().unwrap();
    init_project(&dir);
    set_ns(&dir, "test");

    fs::write(dir.path().join("test.rs"), "fn foo() {}\n").unwrap();

    let batch = r#"[
        {"span": {"path":"test.rs", "start":1, "end":1}},
        {"edge": {"from":"$0", "to":"_", "rel":"risk", "payload":{"summary":"test"}}}
    ]"#;

    let batch_file = dir.path().join("batch.json");
    fs::write(&batch_file, batch).unwrap();

    let output = damask()
        .args(["--format", "json", "batch", "-f", "batch.json"])
        .current_dir(dir.path())
        .output()
        .unwrap();

    assert!(output.status.success());
    let facts: Vec<serde_json::Value> =
        serde_json::from_str(&String::from_utf8(output.stdout).unwrap()).unwrap();
    assert_eq!(facts.len(), 2);
}

#[test]
fn batch_all_or_nothing_on_validation_failure() {
    let dir = TempDir::new().unwrap();
    init_project(&dir);
    set_ns(&dir, "test");

    fs::write(dir.path().join("test.rs"), "x\n").unwrap();

    // First item is valid, second has forward reference — nothing should be written
    let batch = r#"[
        {"span": {"path":"test.rs", "start":1, "end":1}},
        {"edge": {"from":"$5", "to":"_", "rel":"risk", "payload":{"summary":"test"}}}
    ]"#;

    let batch_file = dir.path().join("batch.json");
    fs::write(&batch_file, batch).unwrap();

    damask()
        .args(["batch", "-f", "batch.json"])
        .current_dir(dir.path())
        .assert()
        .failure();

    // No JSONL file should exist
    assert!(!dir.path().join(".damask/edges/test.jsonl").exists());
}

#[test]
fn init_codex_creates_skill() {
    let dir = TempDir::new().unwrap();

    damask()
        .args(["init", "--codex"])
        .current_dir(dir.path())
        .assert()
        .success()
        .stdout(predicate::str::contains("Initialized .damask/"))
        .stdout(predicate::str::contains("Created .agents/skills/damask/SKILL.md"));

    // Verify skill file exists
    assert!(dir.path().join(".agents/skills/damask/SKILL.md").is_file());

    // Verify it contains damask instructions with frontmatter
    let content = fs::read_to_string(dir.path().join(".agents/skills/damask/SKILL.md")).unwrap();
    assert!(content.contains("# Damask"));
    assert!(content.contains("name: damask"));
}

#[test]
fn init_codex_on_existing_project() {
    let dir = TempDir::new().unwrap();
    init_project(&dir);

    // Run init --codex on already-initialized project
    damask()
        .args(["init", "--codex"])
        .current_dir(dir.path())
        .assert()
        .success()
        .stdout(predicate::str::contains("Found existing .damask/"))
        .stdout(predicate::str::contains("Created .agents/skills/damask/SKILL.md"));

    assert!(dir.path().join(".agents/skills/damask/SKILL.md").is_file());
}

#[test]
fn init_codex_idempotent() {
    let dir = TempDir::new().unwrap();

    // First init with --codex
    damask()
        .args(["init", "--codex"])
        .current_dir(dir.path())
        .assert()
        .success();

    // Second run — should detect existing skill
    damask()
        .args(["init", "--codex"])
        .current_dir(dir.path())
        .assert()
        .success()
        .stdout(predicate::str::contains("already current"));

    // Verify # Damask appears exactly once
    let content = fs::read_to_string(dir.path().join(".agents/skills/damask/SKILL.md")).unwrap();
    let count = content.matches("# Damask").count();
    assert_eq!(count, 1, "should have exactly one damask heading");
}

#[test]
fn batch_requires_input_flag() {
    let dir = TempDir::new().unwrap();
    init_project(&dir);
    set_ns(&dir, "test");

    damask()
        .args(["batch"])
        .current_dir(dir.path())
        .assert()
        .failure()
        .stderr(predicate::str::contains("--stdin").or(predicate::str::contains("--file")));
}
