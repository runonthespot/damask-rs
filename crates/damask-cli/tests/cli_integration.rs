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
fn help_shows_all_commands() {
    damask()
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("init"))
        .stdout(predicate::str::contains("span"))
        .stdout(predicate::str::contains("edge"))
        .stdout(predicate::str::contains("at"))
        .stdout(predicate::str::contains("follow"))
        .stdout(predicate::str::contains("endorse"))
        .stdout(predicate::str::contains("dispute"))
        .stdout(predicate::str::contains("tui"));
}
