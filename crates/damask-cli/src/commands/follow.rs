use anyhow::Context;
use damask_core::PayloadEnvelope;
use damask_store::index::query::TraversalChild;
use damask_store::{update_index_with_mode, DamaskProject, IndexMode, IndexQuery, TraversalNode};
use std::env;

use crate::error::Result;
use crate::output::Format;

pub fn run(id: &str, rel: Option<&str>, depth: u32, format: Format) -> Result<()> {
    if !id.starts_with("s_") && !id.starts_with("e_") {
        anyhow::bail!("not a valid span or edge ID: {id}");
    }

    let cwd = env::current_dir().context("failed to get current directory")?;
    let project = DamaskProject::discover(&cwd)
        .map_err(|e| anyhow::anyhow!("{}", e))
        .context("no .damask/ found — run `damask init` first")?;

    let db_path = project.damask_dir.join("index.db");
    let edges_dir = project.damask_dir.join("edges");
    let conn = update_index_with_mode(&db_path, &edges_dir, IndexMode::ViewsPreferred)
        .map_err(|e| anyhow::anyhow!("{}", e))?;

    let q = IndexQuery::new(&conn);
    let tree = q
        .follow(id, rel, depth)
        .map_err(|e| anyhow::anyhow!("{}", e))?;

    match format {
        Format::Human => print_human(&tree),
        Format::Json => print_json(&tree),
    }

    Ok(())
}

fn print_human(tree: &TraversalNode) {
    println!();
    println!("{} ({})", tree.id, tree.display);

    for (i, child) in tree.children.iter().enumerate() {
        let is_last = i == tree.children.len() - 1;
        let prefix = if is_last {
            "\u{2514}\u{2500}\u{2500}"
        } else {
            "\u{251C}\u{2500}\u{2500}"
        };
        print_child(child, prefix, if is_last { "    " } else { "\u{2502}   " });
    }

    if tree.children.is_empty() {
        println!("  (no outgoing edges)");
    }
    println!();
}

fn print_child(child: &TraversalChild, prefix: &str, continuation: &str) {
    let payload: serde_json::Value =
        serde_json::from_str(&child.edge.payload).unwrap_or(serde_json::json!({}));
    let env = PayloadEnvelope::new(&payload);

    let conf = env
        .confidence()
        .map(|c| format!(" ({:.2})", c))
        .unwrap_or_default();

    let date = child.edge.ts.split('T').next().unwrap_or(&child.edge.ts);

    if let Some(ref target) = child.target {
        // Edge points to a span or edge
        let summary = env.summary().map(|s| format!(" {s}")).unwrap_or_default();

        println!(
            "{} {} \u{2192} {} ({}){}{} [{}]",
            prefix, child.edge.rel, target.id, target.display, conf, summary, date,
        );

        // Recurse into target's children
        for (i, grandchild) in target.children.iter().enumerate() {
            let is_last = i == target.children.len() - 1;
            let child_prefix = format!(
                "{}{}",
                continuation,
                if is_last {
                    "\u{2514}\u{2500}\u{2500}"
                } else {
                    "\u{251C}\u{2500}\u{2500}"
                }
            );
            let child_continuation = format!(
                "{}{}",
                continuation,
                if is_last { "    " } else { "\u{2502}   " }
            );
            print_child(grandchild, &child_prefix, &child_continuation);
        }
    } else {
        // Null target — show edge summary as a leaf
        let summary = env
            .summary()
            .unwrap_or_else(|| damask_core::truncate_str(child.edge.payload.as_str(), 60));

        println!(
            "{} {} \u{2192} \"{}\"{} [{}, {}]",
            prefix, child.edge.rel, summary, conf, child.edge.ns, date,
        );
    }
}

fn print_json(tree: &TraversalNode) {
    let json = node_to_json(tree);
    println!("{}", serde_json::to_string_pretty(&json).unwrap());
}

fn node_to_json(node: &TraversalNode) -> serde_json::Value {
    let children: Vec<serde_json::Value> = node
        .children
        .iter()
        .map(|child| {
            let payload: serde_json::Value =
                serde_json::from_str(&child.edge.payload).unwrap_or(serde_json::json!({}));

            let target_json = child.target.as_ref().map(node_to_json);

            serde_json::json!({
                "edge_id": child.edge.id,
                "rel": child.edge.rel,
                "payload": payload,
                "ns": child.edge.ns,
                "ts": child.edge.ts,
                "target": target_json,
            })
        })
        .collect();

    serde_json::json!({
        "id": node.id,
        "kind": format!("{:?}", node.kind).to_lowercase(),
        "display": node.display,
        "children": children,
    })
}
