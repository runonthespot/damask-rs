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
  resolution: string | null;
  recency: string | null;
  ns: string;
  ts: string;
}

/** Traffic-light freshness from the resolver's verdict: green = content
 * hash still matches in place; amber = found but moved, or the file has
 * changed since recording; red = the anchor no longer exists. */
type Freshness = "fresh" | "drifted" | "gone" | "unknown";

function spanFreshness(s: SpanFact | undefined): Freshness {
  if (!s || !s.resolution) return "unknown";
  if (s.resolution === "missing" || s.resolution === "unresolved") return "gone";
  if (s.resolution === "relocated" || s.recency === "file_changed") return "drifted";
  if (s.resolution === "exact") return "fresh";
  return "unknown";
}

const FRESHNESS_DOT: Record<Freshness, string> = {
  fresh: "🟢",
  drifted: "🟠",
  gone: "🔴",
  unknown: "",
};

const FRESHNESS_WORD: Record<Freshness, string> = {
  fresh: "fresh",
  drifted: "code drifted",
  gone: "anchor gone",
  unknown: "",
};

const FRESHNESS_COLOR: Record<Freshness, string> = {
  fresh: "charts.green",
  drifted: "charts.yellow",
  gone: "charts.red",
  unknown: "disabledForeground",
};

/** Worst-of for an edge's anchors: one bad endpoint taints the finding. */
function worstFreshness(anchors: SpanFact[]): Freshness {
  const order: Freshness[] = ["gone", "drifted", "fresh"];
  for (const level of order) {
    if (anchors.some((a) => spanFreshness(a) === level)) return level;
  }
  return "unknown";
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

/** One icon+colour mapping for a rel, used by groups and edges alike.
 * Colour is semantic and sparing: danger red, pitfall orange, judgment
 * purple, structure blue — description-like rels stay unpainted. */
function relIcon(rel: string, closed = false): vscode.ThemeIcon {
  if (closed) {
    return new vscode.ThemeIcon("archive", new vscode.ThemeColor("disabledForeground"));
  }
  const paint = (icon: string, color?: string) =>
    color
      ? new vscode.ThemeIcon(icon, new vscode.ThemeColor(color))
      : new vscode.ThemeIcon(icon);
  switch (rel) {
    case "risk":
      return paint("warning", "charts.red");
    case "gotcha":
      return paint("flame", "charts.orange");
    case "contradicts":
      return paint("git-compare", "charts.red");
    case "decision":
    case "invariant":
      return paint("law", "charts.purple");
    case "depends_on":
    case "co_change":
    case "implements":
      return paint("link", "charts.blue");
    case "describes":
      return paint("book");
    default:
      return paint("circle-outline", "charts.yellow");
  }
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

function isRuledOut(e: EdgeFact): boolean {
  return e.payload?.["status"] === "ruled_out";
}

function tags(e: EdgeFact): string[] {
  const t = e.payload?.["tags"];
  return Array.isArray(t) ? t.filter((x): x is string => typeof x === "string") : [];
}

/** `#security #todo +2` — chips, not a data structure. */
function tagChips(list: string[], max = 3): string {
  if (list.length === 0) return "";
  const shown = list.slice(0, max).map((t) => `#${t}`);
  const extra = list.length > max ? ` +${list.length - max}` : "";
  return " " + shown.join(" ") + extra;
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
      (edge.is_closed ? " (closed)" : "") +
      (isRuledOut(edge) ? " (ruled out)" : "");
    const dot = FRESHNESS_DOT[worstFreshness(anchors)];
    super(
      `${dot ? dot + " " : ""}${summary(edge)}`,
      anchors.length > 0
        ? vscode.TreeItemCollapsibleState.Collapsed
        : vscode.TreeItemCollapsibleState.None
    );
    // Span-links (two anchors) show their wiring inline: a.rs:67 → b.rs:38.
    const base = (p: string) => p.split("/").pop() ?? p;
    const link =
      anchors.length === 2
        ? ` ${base(anchors[0].path)}:${anchors[0].line_start ?? "?"} → ${base(
            anchors[1].path
          )}:${anchors[1].line_start ?? "?"}`
        : "";
    this.description = `${edge.rel}${
      conf !== undefined ? ` ${conf.toFixed(2)}` : ""
    }${marks}${link}${tagChips(tags(edge))}`;
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
    this.iconPath = relIcon(edge.rel, edge.is_closed);
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

/** A namespace — one edge-set folder (.damask/edges/<ns>.jsonl). */
class NamespaceItem extends vscode.TreeItem {
  constructor(public readonly ns: string, public readonly edges: EdgeFact[]) {
    super(ns, vscode.TreeItemCollapsibleState.Collapsed);
    this.description = `${edges.length} edge${edges.length === 1 ? "" : "s"}`;
    this.iconPath = new vscode.ThemeIcon("folder-library");
    this.contextValue = "damaskNamespace";
  }
}

class RelGroupItem extends vscode.TreeItem {
  constructor(public readonly rel: string, public readonly edges: EdgeFact[]) {
    super(`${rel} (${edges.length})`, vscode.TreeItemCollapsibleState.Collapsed);
    this.iconPath = relIcon(rel);
    this.contextValue = "damaskRelGroup";
  }
}

/** A payload field as a tree node — arbitrary shapes welcome: scalars
 * show inline, objects/arrays expand recursively. */
class PayloadItem extends vscode.TreeItem {
  constructor(key: string, public readonly value: unknown) {
    const isContainer =
      value !== null && typeof value === "object";
    super(
      key,
      isContainer
        ? vscode.TreeItemCollapsibleState.Collapsed
        : vscode.TreeItemCollapsibleState.None
    );
    if (isContainer) {
      this.description = Array.isArray(value)
        ? `[${value.length}]`
        : `{${Object.keys(value as object).length}}`;
      this.iconPath = new vscode.ThemeIcon("json");
    } else {
      const text = String(value);
      this.description = text.length > 100 ? text.slice(0, 100) + "…" : text;
      this.tooltip = text;
      this.iconPath = new vscode.ThemeIcon("symbol-field");
    }
    this.contextValue = "damaskPayload";
  }

  children(): PayloadItem[] {
    if (this.value === null || typeof this.value !== "object") return [];
    if (Array.isArray(this.value)) {
      return this.value.map((v, i) => new PayloadItem(String(i), v));
    }
    return Object.entries(this.value as Record<string, unknown>).map(
      ([k, v]) => new PayloadItem(k, v)
    );
  }
}

class AnchorItem extends vscode.TreeItem {
  constructor(public readonly span: SpanFact, role: string) {
    const lines =
      span.line_start != null && span.line_end != null
        ? `:${span.line_start}-${span.line_end}`
        : "";
    super(`${span.path}${lines}`, vscode.TreeItemCollapsibleState.None);
    const fresh = spanFreshness(span);
    const word = FRESHNESS_WORD[fresh];
    this.description = word ? `${role} · ${word}` : role;
    this.iconPath = new vscode.ThemeIcon(
      "circle-filled",
      new vscode.ThemeColor(FRESHNESS_COLOR[fresh])
    );
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
  filterQuery: string | undefined;

  /** Case-insensitive substring match across everything an edge is. */
  private matches(e: EdgeFact): boolean {
    if (!this.filterQuery) return true;
    const q = this.filterQuery.toLowerCase();
    const anchorPaths = [e.from, e.to]
      .filter((id): id is string => !!id)
      .map((id) => this.graph?.spans.get(id)?.path ?? "")
      .join(" ");
    return (
      `${summary(e)} ${e.rel} ${e.ns} ${e.id} ${tags(e).join(" ")} ` +
      `${JSON.stringify(e.payload)} ${anchorPaths}`
    )
      .toLowerCase()
      .includes(q);
  }

  repaint(): void {
    this._onDidChange.fire();
  }

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

  /** Rel-group sections for a set of edges (judgment-first ordering). */
  private relGroups(edges: EdgeFact[]): RelGroupItem[] {
    const byRel = new Map<string, EdgeFact[]>();
    for (const e of edges) {
      const bucket = byRel.get(e.rel);
      if (bucket) bucket.push(e);
      else byRel.set(e.rel, [e]);
    }
    return [...byRel.entries()]
      .sort(
        ([relA, edgesA], [relB, edgesB]) =>
          relRank(relA) - relRank(relB) ||
          edgesB.length - edgesA.length ||
          relA.localeCompare(relB)
      )
      .map(([rel, es]) => new RelGroupItem(rel, es));
  }

  getChildren(element?: vscode.TreeItem): vscode.TreeItem[] {
    if (element instanceof NamespaceItem) {
      return this.relGroups(element.edges);
    }
    if (element instanceof RelGroupItem) {
      // Within a type: confidence descending — best findings first.
      return element.edges
        .sort((a, b) => (confidence(b) ?? 0) - (confidence(a) ?? 0))
        .map((e) => this.edgeItem(e));
    }
    if (element instanceof EdgeItem) {
      const roles =
        element.anchors.length === 2 ? ["from →", "→ to"] : ["anchor"];
      const anchorItems = element.anchors.map(
        (span, i) => new AnchorItem(span, roles[Math.min(i, roles.length - 1)])
      );
      // Payload nested in the tree, not hidden in a hover: every field a
      // node, arbitrary shapes expand recursively. `tags` gets chip
      // treatment instead of array-of-indices ceremony.
      const payloadItems = Object.entries(element.edge.payload ?? {}).map(
        ([k, v]) => {
          if (
            k === "tags" &&
            Array.isArray(v) &&
            v.every((x) => typeof x === "string")
          ) {
            const item = new vscode.TreeItem(
              (v as string[]).map((t) => `#${t}`).join("  ")
            );
            item.iconPath = new vscode.ThemeIcon("tag");
            item.tooltip = `tags: ${(v as string[]).join(", ")}`;
            item.contextValue = "damaskTags";
            return item as PayloadItem;
          }
          return new PayloadItem(k, v);
        }
      );
      return [...anchorItems, ...payloadItems];
    }
    if (element instanceof PayloadItem) {
      return element.children();
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

    // Top level: one folder per namespace (edge set), then rel sections
    // within. A single-namespace store skips the folder ceremony.
    const byNs = new Map<string, EdgeFact[]>();
    let matchCount = 0;
    for (const e of this.graph.edges) {
      if (!this.showClosed && (e.is_closed || isRuledOut(e))) continue;
      if (!this.matches(e)) continue;
      matchCount++;
      const bucket = byNs.get(e.ns);
      if (bucket) bucket.push(e);
      else byNs.set(e.ns, [e]);
    }
    const groups: vscode.TreeItem[] =
      byNs.size === 1
        ? this.relGroups([...byNs.values()][0])
        : [...byNs.entries()]
            .sort(
              ([nsA, edgesA], [nsB, edgesB]) =>
                edgesB.length - edgesA.length || nsA.localeCompare(nsB)
            )
            .map(([ns, edges]) => new NamespaceItem(ns, edges));

    // Active search pinned at the top — click to clear.
    if (this.filterQuery) {
      const filterItem = new vscode.TreeItem(`search: "${this.filterQuery}"`);
      filterItem.description = `${matchCount} match${
        matchCount === 1 ? "" : "es"
      } — click to clear`;
      filterItem.iconPath = new vscode.ThemeIcon("search");
      filterItem.command = {
        command: "damask.clearSearch",
        title: "Clear Search",
      };
      return [filterItem, ...groups];
    }
    return groups;
  }
}

// ---------------------------------------------------------------------------
// Activation
// ---------------------------------------------------------------------------

export function activate(context: vscode.ExtensionContext): void {
  const provider = new DamaskTreeProvider();

  // Closed-edge visibility: stateful, visible, remembered per workspace.
  const applyShowClosed = (value: boolean) => {
    provider.showClosed = value;
    void context.workspaceState.update("damask.showClosed", value);
    void vscode.commands.executeCommand(
      "setContext",
      "damaskShowClosed",
      value
    );
    provider.repaint();
  };
  applyShowClosed(
    context.workspaceState.get<boolean>("damask.showClosed", false)
  );

  context.subscriptions.push(
    vscode.window.createTreeView("damaskGraph", {
      treeDataProvider: provider,
      showCollapseAll: true,
    }),
    vscode.commands.registerCommand("damask.refresh", () => provider.refresh()),
    vscode.commands.registerCommand("damask.showClosed", () =>
      applyShowClosed(true)
    ),
    vscode.commands.registerCommand("damask.hideClosed", () =>
      applyShowClosed(false)
    ),
    vscode.commands.registerCommand("damask.search", async () => {
      const q = await vscode.window.showInputBox({
        prompt: "Search the knowledge graph (summaries, payloads, tags, paths)",
        value: provider.filterQuery ?? "",
        placeHolder: "e.g. race, #security, auth.py",
      });
      if (q !== undefined) {
        provider.filterQuery = q.trim() || undefined;
        provider.repaint();
      }
    }),
    vscode.commands.registerCommand("damask.clearSearch", () => {
      provider.filterQuery = undefined;
      provider.repaint();
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
