//! `glyphtrail import` (#528 spike): upsert externally-sourced nodes/edges —
//! e.g. architecture-graph artifacts — into a repo's existing index, so
//! `query`/`neighbors`/`search` can surface them alongside code symbols
//! without glyphtrail-core having to know what a `Capability` is.

use std::path::Path;

use anyhow::{Context, Result};
use glyphtrail_core::config::RepoPaths;
use glyphtrail_core::{Confidence, Edge, EdgeKind, Node, NodeId, NodeKind};
use serde::Deserialize;

use crate::commands::backend;

#[derive(Deserialize)]
struct ImportNode {
    id: String,
    kind: String,
    name: String,
    #[serde(default)]
    qualified_name: Option<String>,
    #[serde(default)]
    file: String,
    #[serde(default)]
    doc: Option<String>,
}

#[derive(Deserialize)]
struct ImportEdge {
    src: String,
    dst: String,
    kind: String,
}

#[derive(Deserialize, Default)]
struct ImportFile {
    #[serde(default)]
    nodes: Vec<ImportNode>,
    #[serde(default)]
    edges: Vec<ImportEdge>,
}

pub fn run(path: &Path, repo: &Path) -> Result<()> {
    let text =
        std::fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
    let file: ImportFile = serde_json::from_str(&text)
        .with_context(|| format!("parsing {} as import JSON", path.display()))?;

    let root = repo
        .canonicalize()
        .with_context(|| format!("cannot resolve path {}", repo.display()))?;
    let paths = RepoPaths::new(&root);
    paths.ensure_index_dir()?;
    let mut store = backend::open(&paths)?;

    // The caller's own `id` (e.g. `CAP-001`) is used as the `NodeId` directly
    // instead of glyphtrail's usual hash-derived id, so re-running `import`
    // against the same file upserts in place (MERGE on primary key) instead of
    // duplicating, and ids read naturally in `query` output.
    let nodes: Vec<Node> = file
        .nodes
        .iter()
        .map(|n| Node {
            id: NodeId(n.id.clone()),
            kind: NodeKind::parse(&n.kind),
            name: n.name.clone(),
            qualified_name: n
                .qualified_name
                .clone()
                .unwrap_or_else(|| n.name.clone()),
            file: n.file.clone(),
            language: None,
            span: None,
            doc: n.doc.clone(),
            signature: None,
        })
        .collect();
    let edges: Vec<Edge> = file
        .edges
        .iter()
        .map(|e| Edge {
            src: NodeId(e.src.clone()),
            dst: NodeId(e.dst.clone()),
            kind: EdgeKind::parse(&e.kind),
            // Imported facts are authored declarations, not heuristically
            // resolved — the strongest confidence tier fits, and keeps these
            // edges from being pruned by `analyze`'s inferred-confidence cleanup.
            confidence: Confidence::Extracted,
        })
        .collect();

    store.insert_graph(&nodes, &edges)?;
    println!(
        "imported {} node(s) and {} edge(s) from {} into {}",
        nodes.len(),
        edges.len(),
        path.display(),
        root.display()
    );
    Ok(())
}
