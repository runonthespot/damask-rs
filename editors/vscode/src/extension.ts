// Damask for VS Code — a window onto the repo's knowledge graph.
//
// Edges (findings) are top-level; the nodes within each edge are its span
// anchors — click any of them to land on the exact code region. Anchors
// come from the damask index's EFFECTIVE locations, so they stay correct
// through renames and edits. The CLI is the API: everything here is
// `damask log --format json`, parsed and joined client-side.

import * as vscode from "vscode";
import { execFile } from "child_process";
import * as path from "path";

// ---------------------------------------------------------------------------
// Data model (mirrors damask's JSONL facts via `log --format json`)
// ---------------------------------------------------------------------------

interface SpanFact {
  id: string;
  path: string;
  line_start: number | null;
  line_end: number | null;
  ns: string;
  ts: string;
}

interface EdgeFact {
  id: string;
  from: string | null;
  to: string | null;
  rel: string;
  payload: Record<string, unknown>;
  ns: string;
  ts: string;
  is_active: boolean;
  is_closed: boolean;
}

/** Meta-edges are lifecycle signals, not findings — never shown as rows. */
const META_RELS = new Set([
  "endorsed",
  "disputed",
  "closed",
  "supersedes",
  "invalidates",
]);

/** Judgment first — same ordering philosophy as `orient`. */
const REL_ORDER = [
  "risk",
  "gotcha",
  "contradicts",
  "decision",
  "invariant",
  "depends_on",
  "co_change",
  "implements",
  "describes",
];

function relRank(rel: string): number {
  const i = REL_ORDER.indexOf(rel);
  return i === -1 ? REL_ORDER.length : i;
}

interface Graph {
  spans: Map<string, SpanFact>;
  edges: EdgeFact[];
  endorsements: Map<string, number>;
  disputes: Map<string, number>;
}

// ---------------------------------------------------------------------------
// CLI bridge
// ---------------------------------------------------------------------------

function damaskBinary(): string {
  return vscode.workspace.getConfiguration("damask").get("path", "damask");
}

function workspaceRoot(): string | undefined {
  return vscode.workspace.workspaceFolders?.[0]?.uri.fsPath;
}

function runDamaskLog(root: string): Promise<Graph> {
  return new Promise((resolve, reject) => {
    execFile(
      damaskBinary(),
      ["--format", "json", "log", "--limit", "0"],
      { cwd: root, maxBuffer: 64 * 1024 * 1024 },
      (err, stdout) => {
        if (err) {
          reject(err);
          return;
        }
        try {
          const doc = JSON.parse(stdout);
          const spans = new Map<string, SpanFact>();
          const edges: EdgeFact[] = [];
          const endorsements = new Map<string, number>();
          const disputes = new Map<string, number>();
          for (const fact of doc.facts ?? []) {
            if (fact.type === "span") {
              spans.set(fact.id, fact as SpanFact);
            } else if (fact.type === "edge") {
              const e = fact as EdgeFact;
              if (e.rel === "endorsed" && e.from) {
                endorsements.set(e.from, (endorsements.get(e.from) ?? 0) + 1);
              } else if (e.rel === "disputed" && e.from) {
                disputes.set(e.from, (disputes.get(e.from) ?? 0) + 1);
              }
              if (!META_RELS.has(e.rel) && e.is_active) {
                edges.push(e);
              }
            }
          }
          resolve({ spans, edges, endorsements, disputes });
        } catch (parseErr) {
          reject(parseErr);
        }
      }
    );
  });
}

// ---------------------------------------------------------------------------
// Tree items
// ---------------------------------------------------------------------------

function confidence(e: EdgeFact): number | undefined {
  const c = e.payload?.["confidence"];
  return typeof c === "number" ? c : undefined;
}

function summary(e: EdgeFact): string {
  const s = e.payload?.["summary"];
  return typeof s === "string" ? s : JSON.stringify(e.payload).slice(0, 80);
}

class EdgeItem extends vscode.TreeItem {
  constructor(
    public readonly edge: EdgeFact,
    public readonly anchors: SpanFact[],
    endorsed: number,
    disputed: number
  ) {
    const conf = confidence(edge);
    const marks =
      (endorsed > 0 ? ` ×${endorsed}✓` : "") +
      (disputed > 0 ? ` ×${disputed}✗` : "") +
      (edge.is_closed ? " (closed)" : "");
    super(
      `${summary(edge)}`,
      anchors.length > 0
        ? vscode.TreeItemCollapsibleState.Collapsed
        : vscode.TreeItemCollapsibleState.None
    );
    this.description = `${edge.rel}${
      conf !== undefined ? ` ${conf.toFixed(2)}` : ""
    }${marks}`;
    // Full payload, pretty-printed — the hover is the edge's detail view.
    const md = new vscode.MarkdownString(
      `**${edge.rel}** ${conf !== undefined ? `(${conf})` : ""}${marks}\n\n` +
        "```json\n" +
        JSON.stringify(edge.payload, null, 2) +
        "\n```\n\n" +
        `\`${edge.id}\` · ${edge.ns} · ${edge.ts.split("T")[0]}`
    );
    md.supportHtml = false;
    this.tooltip = md;
    this.iconPath = new vscode.ThemeIcon(
      edge.is_closed
        ? "archive"
        : edge.rel === "risk"
        ? "warning"
        : edge.rel === "gotcha"
        ? "flame"
        : edge.rel === "decision" || edge.rel === "invariant"
        ? "law"
        : edge.rel === "describes"
        ? "book"
        : "link"
    );
    // Clicking the edge itself jumps to its primary anchor.
    if (anchors.length > 0) {
      this.command = {
        command: "damask.openAnchor",
        title: "Open Anchor",
        arguments: [anchors[0]],
      };
    }
    this.contextValue = "damaskEdge";
  }
}

class RelGroupItem extends vscode.TreeItem {
  constructor(public readonly rel: string, public readonly edges: EdgeFact[]) {
    super(`${rel} (${edges.length})`, vscode.TreeItemCollapsibleState.Collapsed);
    this.iconPath = new vscode.ThemeIcon(
      rel === "risk"
        ? "warning"
        : rel === "gotcha"
        ? "flame"
        : rel === "decision" || rel === "invariant"
        ? "law"
        : rel === "describes"
        ? "book"
        : rel === "contradicts"
        ? "git-compare"
        : "link"
    );
    this.contextValue = "damaskRelGroup";
  }
}

class AnchorItem extends vscode.TreeItem {
  constructor(public readonly span: SpanFact, role: string) {
    const lines =
      span.line_start != null && span.line_end != null
        ? `:${span.line_start}-${span.line_end}`
        : "";
    super(`${span.path}${lines}`, vscode.TreeItemCollapsibleState.None);
    this.description = role;
    this.iconPath = new vscode.ThemeIcon("location");
    this.tooltip = `${span.id}\n${span.path}${lines}`;
    this.command = {
      command: "damask.openAnchor",
      title: "Open Anchor",
      arguments: [span],
    };
    this.contextValue = "damaskAnchor";
  }
}

// ---------------------------------------------------------------------------
// Tree data provider
// ---------------------------------------------------------------------------

class DamaskTreeProvider implements vscode.TreeDataProvider<vscode.TreeItem> {
  private _onDidChange = new vscode.EventEmitter<void>();
  readonly onDidChangeTreeData = this._onDidChange.event;

  private graph: Graph | undefined;
  private error: string | undefined;
  showClosed = false;

  refresh(): void {
    const root = workspaceRoot();
    if (!root) {
      this.graph = undefined;
      this._onDidChange.fire();
      return;
    }
    runDamaskLog(root).then(
      (g) => {
        this.graph = g;
        this.error = undefined;
        this._onDidChange.fire();
      },
      (err) => {
        this.error = String(err?.message ?? err);
        this._onDidChange.fire();
      }
    );
  }

  getTreeItem(element: vscode.TreeItem): vscode.TreeItem {
    return element;
  }

  private edgeItem(e: EdgeFact): EdgeItem {
    const anchors = [e.from, e.to]
      .filter((id): id is string => !!id && id.startsWith("s_"))
      .map((id) => this.graph!.spans.get(id))
      .filter((s): s is SpanFact => !!s);
    return new EdgeItem(
      e,
      anchors,
      this.graph!.endorsements.get(e.id) ?? 0,
      this.graph!.disputes.get(e.id) ?? 0
    );
  }

  getChildren(element?: vscode.TreeItem): vscode.TreeItem[] {
    if (element instanceof RelGroupItem) {
      // Within a type: confidence descending — best findings first.
      return element.edges
        .sort((a, b) => (confidence(b) ?? 0) - (confidence(a) ?? 0))
        .map((e) => this.edgeItem(e));
    }
    if (element instanceof EdgeItem) {
      const roles =
        element.anchors.length === 2 ? ["from", "to"] : ["anchor"];
      return element.anchors.map(
        (span, i) => new AnchorItem(span, roles[Math.min(i, roles.length - 1)])
      );
    }
    if (element) {
      return [];
    }
    if (this.error) {
      const item = new vscode.TreeItem(
        this.error.includes("ENOENT")
          ? "damask CLI not found — set damask.path in settings"
          : `damask error: ${this.error.slice(0, 120)}`
      );
      item.iconPath = new vscode.ThemeIcon("error");
      return [item];
    }
    if (!this.graph) {
      return [new vscode.TreeItem("No .damask/ graph in this workspace")];
    }

    // Top level: one group per edge type — judgment rels first, ties
    // broken by size, mirroring orient's sectioning.
    const byRel = new Map<string, EdgeFact[]>();
    for (const e of this.graph.edges) {
      if (!this.showClosed && e.is_closed) continue;
      const bucket = byRel.get(e.rel);
      if (bucket) bucket.push(e);
      else byRel.set(e.rel, [e]);
    }
    return [...byRel.entries()]
      .sort(
        ([relA, edgesA], [relB, edgesB]) =>
          relRank(relA) - relRank(relB) || edgesB.length - edgesA.length
      )
      .map(([rel, edges]) => new RelGroupItem(rel, edges));
  }
}

// ---------------------------------------------------------------------------
// Activation
// ---------------------------------------------------------------------------

export function activate(context: vscode.ExtensionContext): void {
  const provider = new DamaskTreeProvider();
  context.subscriptions.push(
    vscode.window.createTreeView("damaskGraph", {
      treeDataProvider: provider,
      showCollapseAll: true,
    }),
    vscode.commands.registerCommand("damask.refresh", () => provider.refresh()),
    vscode.commands.registerCommand("damask.showClosed", () => {
      provider.showClosed = !provider.showClosed;
      provider.refresh();
    }),
    vscode.commands.registerCommand(
      "damask.openAnchor",
      async (span: SpanFact) => {
        const root = workspaceRoot();
        if (!root) return;
        const file = path.isAbsolute(span.path)
          ? span.path
          : path.join(root, span.path);
        try {
          const doc = await vscode.workspace.openTextDocument(file);
          const editor = await vscode.window.showTextDocument(doc);
          const startLine = Math.max(0, (span.line_start ?? 1) - 1);
          const endLine = Math.max(startLine, (span.line_end ?? 1) - 1);
          const range = new vscode.Range(
            startLine,
            0,
            endLine,
            doc.lineAt(Math.min(endLine, doc.lineCount - 1)).text.length
          );
          editor.selection = new vscode.Selection(range.start, range.end);
          editor.revealRange(range, vscode.TextEditorRevealType.InCenter);
        } catch {
          vscode.window.showWarningMessage(
            `Damask: anchor file not found — ${span.path} (the finding may need triage)`
          );
        }
      }
    )
  );

  // Live refresh: the store is append-only JSONL, so any change to
  // .damask/edges is a new fact landing.
  const watcher = vscode.workspace.createFileSystemWatcher(
    "**/.damask/edges/*.jsonl"
  );
  let debounce: NodeJS.Timeout | undefined;
  const kick = () => {
    if (debounce) clearTimeout(debounce);
    debounce = setTimeout(() => provider.refresh(), 400);
  };
  watcher.onDidChange(kick);
  watcher.onDidCreate(kick);
  watcher.onDidDelete(kick);
  context.subscriptions.push(watcher);

  provider.refresh();
}

export function deactivate(): void {}
