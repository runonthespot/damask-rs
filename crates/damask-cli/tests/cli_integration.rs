use assert_cmd::Command;
use predicates::prelude::*;
use std::fs;
use tempfile::TempDir;

fn damask() -> Command {
    Command::cargo_bin("damask").unwrap()
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
fn ns_required_for_span() {
    let dir = TempDir::new().unwrap();
    init_project(&dir);
    // Don't set a namespace

    fs::write(dir.path().join("test.rs"), "x\n").unwrap();

    damask()
        .args(["span", "test.rs", "1", "1"])
        .current_dir(dir.path())
        .assert()
        .failure()
        .stderr(predicate::str::contains("no active namespace"));
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
        .stdout(predicate::str::contains("No edges matching"));
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
        .stdout(predicate::str::contains("Claude Code skill created"));

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
        .stdout(predicate::str::contains("already exists"));

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
