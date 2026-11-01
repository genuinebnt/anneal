// Query plans from several databases in one shape, for the plan explorer and the plans on SQL course pages.
//
// Parsed: Postgres EXPLAIN (FORMAT JSON) and the text format, MySQL EXPLAIN ANALYZE / EXPLAIN FORMAT=TREE, and SQLite
// EXPLAIN QUERY PLAN. Every node gets its own time and pages (inclusive figures minus its children's), and flags for
// what usually matters: a big misestimate, rows thrown away by a filter, a sort that spilled to disk, a hash that ran in
// batches, heap fetches in an index-only scan.

export type Engine = "postgres" | "mysql" | "sqlite";

/** A captured query plan, as tools/sqlperf/capture.py wrote it (`content/sql/plans/<id>.json`). */
export interface PlanFile {
    id: string;
    engine: string;
    version: string;
    query: string;
    setup: { id: string; sql: string }[];
    json: unknown;
    text: string;
}

export interface PlanNode {
    op: string;
    /** The table, index, CTE or function the node reads, when it reads one. */
    on?: string;
    index?: string;
    /** Conditions and other details, in the engine's words: [label, text]. */
    details: [string, string][];
    estRows?: number;
    rows?: number;
    loops?: number;
    /** Milliseconds for all loops, including children; and the node's own share. */
    totalMs?: number;
    selfMs?: number;
    /** Pages (blocks) touched, including children; and the node's own share. */
    pages?: number;
    selfPages?: number;
    pagesRead?: number;
    removed?: number;
    flags: Flag[];
    children: PlanNode[];
    depth: number;
}

export interface Flag {
    tone: "bad" | "warn" | "info";
    text: string;
}

export interface Plan {
    engine: Engine;
    root: PlanNode;
    planningMs?: number;
    executionMs?: number;
    /** Every node, depth first, for lists and lookups. */
    nodes: PlanNode[];
}

const num = (v: unknown) => (typeof v === "number" && Number.isFinite(v) ? v : undefined);

/** Fills in own time and pages, flags and depth, after a parser has built the tree. */
function finish(engine: Engine, root: PlanNode, planningMs?: number, executionMs?: number): Plan {
    const nodes: PlanNode[] = [];
    const walk = (n: PlanNode, depth: number) => {
        n.depth = depth;
        nodes.push(n);
        n.children.forEach((c) => walk(c, depth + 1));
        const kidsMs = n.children.reduce((a, c) => a + (c.totalMs ?? 0), 0);
        if (n.totalMs !== undefined) n.selfMs = Math.max(0, n.totalMs - kidsMs);
        const kidsPages = n.children.reduce((a, c) => a + (c.pages ?? 0), 0);
        if (n.pages !== undefined) n.selfPages = Math.max(0, n.pages - kidsPages);
        if (n.estRows !== undefined && n.rows !== undefined && n.rows + n.estRows > 0) {
            const ratio = Math.max(n.rows, 1) / Math.max(n.estRows, 1);
            if (ratio >= 10 || ratio <= 0.1) n.flags.push({ tone: "warn", text: `estimate off by ${ratio >= 1 ? Math.round(ratio) : Math.round(1 / ratio)}×: planned ${fmt(n.estRows)} rows, got ${fmt(n.rows)}` });
        }
        if (n.removed && n.rows !== undefined && n.removed > Math.max(10, n.rows * 10)) n.flags.push({ tone: "bad", text: `${fmt(n.removed)} rows read and thrown away by the filter for ${fmt(n.rows)} kept` });
    };
    walk(root, 0);
    return { engine, root, planningMs, executionMs, nodes };
}

export const fmt = (n: number) => (n >= 1e6 ? `${(n / 1e6).toFixed(n >= 1e7 ? 0 : 1)}M` : n >= 1e4 ? `${Math.round(n / 1e3)}k` : Number.isInteger(n) ? n.toLocaleString("en") : n.toFixed(n < 10 ? 2 : 1));

// ---------------------------------------------------------------- Postgres JSON

const AGG_STRATEGY: Record<string, string> = { Sorted: "GroupAggregate", Hashed: "HashAggregate", Mixed: "MixedAggregate" };

/** The node's name as EXPLAIN's text format prints it: JSON keeps an aggregate's strategy and partial mode in their own fields. */
function jsonNodeName(p: Record<string, unknown>): string {
    const type = String(p["Node Type"] ?? "?");
    if (type !== "Aggregate") return type;
    const name = AGG_STRATEGY[String(p["Strategy"])] ?? type;
    const mode = p["Partial Mode"];
    return mode === "Partial" || mode === "Finalize" ? `${mode} ${name}` : name;
}

/** EXPLAIN (ANALYZE, BUFFERS, FORMAT JSON): the richest input; times are per loop, so totals multiply by loops. */
export function fromPostgresJson(input: unknown): Plan {
    const top = (Array.isArray(input) ? input[0] : input) as Record<string, unknown>;
    if (!top || typeof top !== "object" || !("Plan" in top)) throw new Error("not a Postgres JSON plan: expected an object with a \"Plan\" key");
    // inside a Gather, a node's time is the average over the worker processes (its loops), so it is not multiplied by
    // them; rows are summed over the processes either way
    const conv = (p: Record<string, unknown>, parallel = false): PlanNode => {
        const loops = num(p["Actual Loops"]) ?? 1;
        const perLoop = num(p["Actual Total Time"]);
        const underGather = parallel || /^Gather/.test(String(p["Node Type"]));
        const details: [string, string][] = [];
        const keys = ["Index Cond", "Recheck Cond", "Filter", "Join Filter", "Hash Cond", "Merge Cond", "Sort Key", "Group Key", "Presorted Key", "Sort Method", "Strategy", "Join Type", "Heap Fetches", "Exact Heap Blocks", "Lossy Heap Blocks", "Index Searches", "Workers Planned", "Workers Launched", "Hash Batches", "Original Hash Batches", "Peak Memory Usage", "Sort Space Used", "Sort Space Type", "Cache Hits", "Cache Misses", "Output"];
        for (const k of keys) {
            const v = p[k];
            if (v === undefined || v === null) continue;
            details.push([k, Array.isArray(v) ? v.join(", ") : String(v)]);
        }
        const hit = num(p["Shared Hit Blocks"]) ?? 0;
        const read = num(p["Shared Read Blocks"]) ?? 0;
        const hasBuffers = "Shared Hit Blocks" in p;
        const flags: Flag[] = [];
        if (p["Sort Space Type"] === "Disk") flags.push({ tone: "bad", text: `the sort spilled to disk (${p["Sort Space Used"]} kB): raise work_mem for this query, or sort fewer rows` });
        if ((num(p["Hash Batches"]) ?? 1) > 1) flags.push({ tone: "warn", text: `the hash table ran in ${p["Hash Batches"]} batches because it did not fit in work_mem` });
        if (p["Node Type"] === "Index Only Scan" && (num(p["Heap Fetches"]) ?? 0) > 0) flags.push({ tone: "warn", text: `${fmt(num(p["Heap Fetches"])!)} heap fetches: the visibility map is stale, so the "index-only" scan still read the table (VACUUM fixes it)` });
        if ((num(p["Workers Launched"]) ?? 0) < (num(p["Workers Planned"]) ?? 0)) flags.push({ tone: "warn", text: `only ${p["Workers Launched"]} of ${p["Workers Planned"]} parallel workers started` });
        if ((num(p["Lossy Heap Blocks"]) ?? 0) > 0) flags.push({ tone: "info", text: "the bitmap went lossy: whole pages were rechecked because work_mem was too small to keep exact row positions" });
        const removed = (num(p["Rows Removed by Filter"]) ?? 0) + (num(p["Rows Removed by Join Filter"]) ?? 0) + (num(p["Rows Removed by Index Recheck"]) ?? 0);
        const children = ((p["Plans"] as Record<string, unknown>[] | undefined) ?? []).map((c) => conv(c, underGather));
        return {
            op: (p["Parallel Aware"] ? "Parallel " : "") + jsonNodeName(p) + (p["Scan Direction"] === "Backward" ? " Backward" : ""),
            on: (p["Relation Name"] as string) ?? (p["CTE Name"] as string) ?? (p["Function Name"] as string),
            index: p["Index Name"] as string | undefined,
            details,
            // Plan Rows and Actual Rows are both per loop (per worker process under a Gather); show totals for both
            estRows: num(p["Plan Rows"]) !== undefined ? Math.round(num(p["Plan Rows"])! * loops) : undefined,
            rows: num(p["Actual Rows"]) !== undefined ? Math.round(num(p["Actual Rows"])! * loops) : undefined,
            loops,
            totalMs: perLoop !== undefined ? perLoop * (parallel ? 1 : loops) : undefined,
            pages: hasBuffers ? hit + read : undefined,
            pagesRead: hasBuffers ? read : undefined,
            removed: removed ? removed * loops : undefined,
            flags,
            children,
            depth: 0,
        };
    };
    return finish("postgres", conv(top["Plan"] as Record<string, unknown>), num(top["Planning Time"]), num(top["Execution Time"]));
}

// ---------------------------------------------------------------- Postgres text

/** The default text format, with or without ANALYZE and BUFFERS. Nodes start with "->" (except the first); detail lines follow. */
export function fromPostgresText(text: string): Plan {
    const lines = text.split("\n").filter((l) => l.trim() && !/^\s*QUERY PLAN\s*$/.test(l) && !/^-+$/.test(l.trim()) && !/^\(\d+ rows?\)$/.test(l.trim()));
    let planningMs: number | undefined;
    let executionMs: number | undefined;
    const stack: { indent: number; node: PlanNode }[] = [];
    let root: PlanNode | undefined;
    let inPlanning = false;
    for (const raw of lines) {
        const line = raw.replace(/\s+$/, "");
        const pm = /^\s*Planning Time:\s*([\d.]+)/.exec(line);
        if (pm) { planningMs = Number(pm[1]); continue; }
        const em = /^\s*Execution Time:\s*([\d.]+)/.exec(line);
        if (em) { executionMs = Number(em[1]); continue; }
        if (/^\s*Planning:\s*$/.test(line)) { inPlanning = true; continue; }
        const nodeMatch = /^(\s*)(?:->\s+)?([A-Z][A-Za-z ]+?)(?:\s+(?:on|using)\s+(.+?))?\s+\((?:cost=([\d.]+)\.\.([\d.]+)\s+rows=(\d+)\s+width=\d+\))?\s*(?:\(actual(?: time=([\d.]+)\.\.([\d.]+))? rows=([\d.]+) loops=(\d+)\)|\(never executed\))?/.exec(line);
        const isNode = nodeMatch && (/->/.test(line) || !root) && /\(cost=|\(actual|\(never executed\)/.test(line);
        if (isNode && nodeMatch) {
            inPlanning = false;
            const indent = line.indexOf(line.trim());
            const loops = nodeMatch[10] ? Number(nodeMatch[10]) : undefined;
            let on: string | undefined;
            let index: string | undefined;
            const target = nodeMatch[3];
            if (target) {
                const u = /^(\S+)\s+on\s+(\S+)/.exec(target);
                if (u) { index = u[1]; on = u[2]; } else if (/^Bitmap Index Scan/.test(nodeMatch[2]!.trim())) index = target.split(/\s+/)[0];
                else on = target.split(/\s+/)[0];
            }
            const node: PlanNode = {
                op: nodeMatch[2]!.trim(),
                on,
                index,
                details: [],
                estRows: nodeMatch[6] ? Math.round(Number(nodeMatch[6]) * (loops ?? 1)) : undefined,
                rows: nodeMatch[9] && loops ? Math.round(Number(nodeMatch[9]) * loops) : undefined,
                loops,
                totalMs: nodeMatch[8] && loops ? Number(nodeMatch[8]) * loops : undefined,
                flags: [],
                children: [],
                depth: 0,
            };
            while (stack.length && stack[stack.length - 1]!.indent >= indent) stack.pop();
            if (stack.length) stack[stack.length - 1]!.node.children.push(node);
            else root = node;
            stack.push({ indent, node });
            continue;
        }
        if (inPlanning || !stack.length) continue;
        const cur = stack[stack.length - 1]!.node;
        const kv = /^\s*([A-Z][A-Za-z ]+?):\s*(.*)$/.exec(line);
        if (!kv) continue;
        const [, k, v] = kv;
        if (k === "Buffers") {
            const hit = Number(/hit=(\d+)/.exec(v!)?.[1] ?? 0);
            const read = Number(/read=(\d+)/.exec(v!)?.[1] ?? 0);
            cur.pages = hit + read;
            cur.pagesRead = read;
        } else if (/^Rows Removed by/.test(k!)) {
            cur.removed = (cur.removed ?? 0) + Number(v) * (cur.loops ?? 1);
        } else {
            cur.details.push([k!, v!]);
            if (k === "Sort Method" && /Disk/.test(v!)) cur.flags.push({ tone: "bad", text: `the sort spilled to disk (${v}): raise work_mem for this query, or sort fewer rows` });
            if (k === "Heap Fetches" && Number(v) > 0 && cur.op.startsWith("Index Only Scan")) cur.flags.push({ tone: "warn", text: `${v} heap fetches: the visibility map is stale, so the "index-only" scan still read the table (VACUUM fixes it)` });
        }
    }
    if (!root) throw new Error("no plan nodes found: paste the whole EXPLAIN output");
    return finish("postgres", root, planningMs, executionMs);
}

// ---------------------------------------------------------------- MySQL

/** MySQL 8 EXPLAIN ANALYZE and EXPLAIN FORMAT=TREE: one node per "-> " line, nested by indentation. */
export function fromMysqlTree(text: string): Plan {
    const stack: { indent: number; node: PlanNode }[] = [];
    let root: PlanNode | undefined;
    for (const raw of text.split("\n")) {
        const at = raw.indexOf("-> ");
        if (at < 0) continue;
        const body = raw.slice(at + 3).replace(/\s*\|?\s*$/, "");
        const cost = /\(cost=([\d.e+]+)(?:\.\.([\d.e+]+))? rows=([\d.e+]+)\)/.exec(body);
        const actual = /\(actual time=([\d.]+)\.\.([\d.]+) rows=([\d.e+]+) loops=(\d+)\)/.exec(body);
        const label = body.replace(/\s*\(cost=.*$/, "").replace(/\s*\(actual.*$/, "").trim();
        const [opPart, ...rest] = label.split(": ");
        const desc = rest.join(": ");
        const onMatch = /(?:on|of)\s+(\w+)(?:\s+using\s+(\w+))?/.exec(label);
        const loops = actual ? Number(actual[4]) : undefined;
        const node: PlanNode = {
            op: opPart!.trim(),
            on: onMatch?.[1],
            index: onMatch?.[2],
            details: desc ? [["Detail", desc]] : [],
            estRows: cost ? Number(cost[3]) : undefined,
            rows: actual && loops ? Number(actual[3]) * loops : undefined,
            loops,
            totalMs: actual && loops ? Number(actual[2]) * loops : undefined,
            flags: [],
            children: [],
            depth: 0,
        };
        while (stack.length && stack[stack.length - 1]!.indent >= at) stack.pop();
        if (stack.length) stack[stack.length - 1]!.node.children.push(node);
        else root = node;
        stack.push({ indent: at, node });
    }
    if (!root) throw new Error("no MySQL plan lines found: paste the output of EXPLAIN ANALYZE or EXPLAIN FORMAT=TREE");
    return finish("mysql", root);
}

// ---------------------------------------------------------------- SQLite

/** sqlite3's EXPLAIN QUERY PLAN tree ("QUERY PLAN", then "|--" and "`--" lines). It has no row counts or timings. */
export function fromSqlite(text: string): Plan {
    const root: PlanNode = { op: "QUERY PLAN", details: [], flags: [], children: [], depth: 0 };
    const stack: { indent: number; node: PlanNode }[] = [{ indent: -1, node: root }];
    for (const raw of text.split("\n")) {
        const m = /^([\s|]*)(?:\|--|`--)(.*)$/.exec(raw);
        if (!m) continue;
        const indent = m[1]!.length;
        const body = m[2]!.trim();
        const scan = /^(SCAN|SEARCH)\s+(\w+)(?:\s+USING\s+(COVERING INDEX|INDEX|INTEGER PRIMARY KEY|PRIMARY KEY)\s*(\w+)?\s*(\(.*\))?)?/.exec(body);
        const node: PlanNode = {
            op: scan ? `${scan[1]}${scan[3] ? ` using ${scan[3].toLowerCase()}` : ""}` : body,
            on: scan?.[2],
            index: scan?.[4],
            details: scan?.[5] ? [["Condition", scan[5]]] : [],
            flags: [],
            children: [],
            depth: 0,
        };
        if (/^SCAN \w+$/.test(body)) node.flags.push({ tone: "warn", text: "a full scan of the table (no index used)" });
        if (/USE TEMP B-TREE/.test(body)) node.flags.push({ tone: "warn", text: "a temporary B-tree is built to sort or group: an index in that order would avoid it" });
        while (stack.length > 1 && stack[stack.length - 1]!.indent >= indent) stack.pop();
        stack[stack.length - 1]!.node.children.push(node);
        stack.push({ indent, node });
    }
    if (!root.children.length) throw new Error("no SQLite plan lines found: paste the output of EXPLAIN QUERY PLAN");
    return finish("sqlite", root.children.length === 1 ? root.children[0]! : root);
}

/** Guesses the format and parses it. */
export function parsePlan(text: string): Plan {
    const t = text.trim();
    if (t.startsWith("[") || t.startsWith("{")) return fromPostgresJson(JSON.parse(t));
    if (/^\s*(QUERY PLAN\s*)?\n?\s*(\||`)--/m.test(t) || /^QUERY PLAN\s*\n\s*[|`]--/m.test(t)) return fromSqlite(t);
    if (/^\s*->\s/m.test(t) && /\(cost=[\d.e+]+(\.\.[\d.e+]+)? rows=[\d.e+]+\)|\(actual time=[\d.]+\.\.[\d.]+ rows=[\d.]+ loops=\d+\)/.test(t) && !/width=\d+/.test(t)) return fromMysqlTree(t);
    return fromPostgresText(t);
}

// ---------------------------------------------------------------- what each operation is

export interface OpInfo {
    what: string;
    good: string;
    watch: string;
    /** The SQL course page that explains it. */
    page?: string;
}

const KB: [RegExp, OpInfo][] = [
    [/^(Parallel )?Seq Scan/, { what: "Reads every page of the table, in physical order, and checks each row against the filter.", good: "The cheapest way to read a large share of a table: sequential reads, no index overhead.", watch: "On a big table for a query that returns few rows: an index is missing or cannot be used (a function on the column, a type mismatch, a leading column not constrained).", page: "sql-anatomy" }],
    [/^Index Only Scan/, { what: "Answers the query from the index alone, visiting the table only for pages the visibility map does not mark all-visible.", good: "The best case: no table pages at all when Heap Fetches is 0.", watch: "Heap Fetches above zero means VACUUM has not run recently enough on the table.", page: "sql-multicolumn" }],
    [/^Index Scan/, { what: "Walks the index to the matching entries, then reads the table row for each, in index order.", good: "Few rows, or rows needed in index order (ORDER BY ... LIMIT).", watch: "Many matching rows: each can be a different table page. Rows Removed by Filter means the index finds more than the query keeps.", page: "sql-equality" }],
    [/^Bitmap Index Scan/, { what: "Collects the positions of all matching entries from the index into an in-memory bitmap, without touching the table.", good: "A moderate number of rows: the bitmap lets the next step read each table page once, in physical order.", watch: "Combined in BitmapAnd or BitmapOr when several indexes help one query; a single composite index is usually faster.", page: "sql-equality" }],
    [/^Bitmap Heap Scan/, { what: "Reads the table pages named by the bitmap, in physical order, each page once, and rechecks the condition.", good: "Between Index Scan and Seq Scan: too many rows for one-by-one lookups, too few for reading everything.", watch: "Lossy Heap Blocks means work_mem was too small to keep exact row positions, so whole pages were rechecked.", page: "sql-equality" }],
    [/^Gather Merge/, { what: "Collects sorted rows from parallel workers and merges them, keeping the order.", good: "Lets a sort run in parallel.", watch: "Fewer workers launched than planned (the server's limits).", page: "sql-anatomy" }],
    [/^Gather/, { what: "Collects rows from parallel workers running the plan below it.", good: "Big scans and aggregates finish faster on several cores.", watch: "Fewer workers launched than planned; for queries returning few rows, parallel startup is pure overhead.", page: "sql-anatomy" }],
    [/^Incremental Sort/, { what: "Sorts rows that are already sorted by a leading key, one group at a time.", good: "Cheaper than a full sort and can stop early with LIMIT.", watch: "Many tiny groups behave like a full sort." }],
    [/^Sort/, { what: "Sorts all its input rows before returning the first.", good: "Small inputs in memory (quicksort, or top-N heapsort under a LIMIT).", watch: "external merge or Disk means it spilled; an index in the same order avoids sorting at all.", page: "sql-multicolumn" }],
    [/^Limit/, { what: "Stops after the first N rows.", good: "With an index that returns rows in the wanted order, the plan below stops early.", watch: "Above a Sort, every row is still read and sorted first." }],
    [/^Nested Loop/, { what: "For each row from the outer (first) child, runs the inner child.", good: "Few outer rows and an index on the inner side's join column.", watch: "A large outer side with an inner scan that is not an index lookup: the classic N+1 inside the database." }],
    [/^Hash Join/, { what: "Builds a hash table from the inner (second) child, then probes it with each outer row.", good: "Joining large sets on equality, with no useful index.", watch: "Batches above 1 mean the hash table spilled to disk; check the inner side's row estimate." }],
    [/^Merge Join/, { what: "Walks two inputs sorted by the join key side by side, like a zipper.", good: "Both sides already sorted, for example by indexes on the join columns.", watch: "Explicit Sort nodes below it that cost more than a hash join would." }],
    [/^Hash$/, { what: "Builds the in-memory hash table for the Hash Join above it.", good: "Small inputs that fit in work_mem.", watch: "Batches above 1, or a large underestimate of its rows." }],
    [/^Aggregate|^HashAggregate|^GroupAggregate|^Finalize|^Partial/, { what: "Computes aggregates (count, sum, ...), by hashing groups or by walking rows already sorted by the group key.", good: "Hashed for many unsorted rows; sorted (group) when input arrives in group order.", watch: "A hashed aggregate that spills to disk (Disk Usage, Batches)." }],
    [/^Memoize/, { what: "Caches the inner side of a nested loop by its parameters, so repeated lookups of the same value are answered from memory.", good: "Many outer rows that repeat the same join keys.", watch: "A low hit ratio (Cache Misses close to rows) makes it pure overhead." }],
    [/^Materialize/, { what: "Stores its child's rows in memory so they can be read again.", good: "The inner side of a nested loop read many times.", watch: "Large inputs spill to disk." }],
    [/^(Append|Merge Append)/, { what: "Reads several child plans one after another (partitions, UNION ALL); Merge Append keeps a sort order.", good: "Partition pruning leaves only the partitions that can match.", watch: "Every partition listed: the filter could not prune them." }],
    [/^WindowAgg/, { what: "Computes window functions over partitions of sorted rows.", good: "Input already sorted by the window's PARTITION BY and ORDER BY.", watch: "A Sort below it on a large input." }],
    [/^(Insert|Update|Delete|Merge|ModifyTable)/, { what: "Writes rows: every index on the table is maintained as well.", good: "Batched writes.", watch: "Slow writes often come from many indexes or from the scan below finding the rows." }],
    // MySQL
    [/^Table scan/i, { what: "MySQL reads every row of the table.", good: "Small tables, or most rows needed.", watch: "A missing or unusable index." }],
    [/^Covering index (lookup|scan|range scan)/i, { what: "MySQL answers from the secondary index alone, without the clustered index.", good: "The fastest lookup shape in InnoDB.", watch: "" }],
    [/^Index (lookup|range scan|scan)/i, { what: "MySQL walks a secondary index, then the clustered primary-key index for each row.", good: "Few rows.", watch: "Many rows: two B-tree descents per row." }],
    [/^Single-row index lookup/i, { what: "MySQL looks up one row by a unique key.", good: "The ideal point lookup.", watch: "" }],
    [/^Filter/i, { what: "MySQL checks a condition on rows from the node below.", good: "Cheap conditions on few rows.", watch: "Many rows filtered away: the condition is not used by an index." }],
    [/join/i, { what: "MySQL joins the rows of its two children.", good: "An index on the inner side's join column (nested loop), or hash join for large unindexed equality joins.", watch: "Nested loops over table scans." }],
    // SQLite
    [/^SEARCH using covering index/i, { what: "SQLite answers from the index alone.", good: "No table lookups.", watch: "" }],
    [/^SEARCH/i, { what: "SQLite uses an index (or the rowid) to find only the matching rows.", good: "Selective conditions.", watch: "" }],
    [/^SCAN/i, { what: "SQLite reads the whole table or index.", good: "Small tables.", watch: "A missing index for a selective condition." }],
];

export function opInfo(op: string): OpInfo | undefined {
    return KB.find(([re]) => re.test(op))?.[1];
}

/** The node that cost the most, by own time when timings exist, else by own pages. */
export function hottest(plan: Plan): PlanNode | undefined {
    const byTime = plan.nodes.filter((n) => n.selfMs !== undefined);
    if (byTime.length) return byTime.reduce((a, b) => ((b.selfMs ?? 0) > (a.selfMs ?? 0) ? b : a));
    const byPages = plan.nodes.filter((n) => n.selfPages !== undefined);
    return byPages.length ? byPages.reduce((a, b) => ((b.selfPages ?? 0) > (a.selfPages ?? 0) ? b : a)) : undefined;
}
