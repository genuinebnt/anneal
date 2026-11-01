import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";
import { fromMysqlTree, fromPostgresJson, fromPostgresText, fromSqlite, hottest, opInfo, parsePlan } from "./plan.ts";

const captured = (id: string) => JSON.parse(readFileSync(new URL(`../../content/sql/plans/${id}.json`, import.meta.url), "utf8"));

test("a captured Postgres plan parses the same from JSON and from text", () => {
    const file = captured("m1-customer-lookup");
    const j = fromPostgresJson(file.json);
    const t = fromPostgresText(file.text);
    assert.equal(j.root.op, "Bitmap Heap Scan");
    assert.equal(t.root.op, "Bitmap Heap Scan");
    assert.equal(j.root.children[0]!.op, "Bitmap Index Scan");
    assert.equal(t.root.children[0]!.index, "orders_customer");
    assert.equal(j.root.pages, t.root.pages);
    assert.equal(j.root.rows, t.root.rows);
    // own pages: the heap scan's pages minus the index scan's
    assert.equal(j.root.selfPages, j.root.pages! - j.root.children[0]!.pages!);
});

test("a parallel scan is the hottest node, and its time is not multiplied by the workers", () => {
    const p = fromPostgresJson(captured("m1-customer-no-index").json);
    const scan = p.nodes.find((n) => n.op === "Parallel Seq Scan")!;
    assert.equal(hottest(p), scan);
    assert.ok(scan.totalMs! <= p.executionMs!, "a worker's average time cannot exceed the query's");
    assert.ok(scan.removed! > 1_000_000, "rows removed by the filter are summed over the processes");
    assert.ok(scan.flags.some((f) => f.tone === "bad"));
});

test("MySQL EXPLAIN ANALYZE nests by indentation and keeps rows and loops", () => {
    const p = fromMysqlTree(`-> Limit: 20 row(s)  (cost=12.5 rows=20) (actual time=0.05..0.12 rows=20 loops=1)
    -> Index lookup on orders using orders_customer (customer_id=4242)  (cost=12.5 rows=44) (actual time=0.04..0.11 rows=20 loops=1)`);
    assert.equal(p.root.op, "Limit");
    assert.equal(p.root.children[0]!.op, "Index lookup on orders using orders_customer (customer_id=4242)");
    assert.equal(p.root.children[0]!.on, "orders");
    assert.equal(p.root.children[0]!.index, "orders_customer");
    assert.equal(p.root.children[0]!.rows, 20);
    assert.ok(opInfo(p.root.children[0]!.op));
});

test("SQLite EXPLAIN QUERY PLAN: scans, searches and temp B-trees", () => {
    const p = fromSqlite(`QUERY PLAN
|--SEARCH orders USING INDEX orders_customer (customer_id=?)
\`--USE TEMP B-TREE FOR ORDER BY`);
    assert.equal(p.root.op, "QUERY PLAN");
    assert.equal(p.root.children[0]!.on, "orders");
    assert.equal(p.root.children[0]!.index, "orders_customer");
    assert.ok(p.root.children[1]!.flags.length > 0);
});

test("the format is guessed from the text", () => {
    assert.equal(parsePlan(JSON.stringify([captured("m1-pk-lookup").json])).engine, "postgres");
    assert.equal(parsePlan(captured("m1-pk-lookup").text).root.op, "Index Scan");
    assert.equal(parsePlan("QUERY PLAN\n`--SCAN orders").engine, "sqlite");
    assert.equal(parsePlan("-> Table scan on orders  (cost=2e+05 rows=2e+06) (actual time=0.1..300 rows=2e+06 loops=1)").engine, "mysql");
});

test("planned rows are totals over loops, like the actual rows, so a parallel scan is not called a misestimate", () => {
    const p = fromPostgresJson(captured("m1-customer-no-index").json);
    const scan = p.nodes.find((n) => n.op === "Parallel Seq Scan")!;
    assert.equal(scan.loops, 3);
    assert.ok(Math.abs(scan.estRows! - scan.rows!) < scan.rows!, "the estimate (17 per worker, 51 in all) is close to the 44 found");
    assert.ok(!scan.flags.some((f) => /estimate off/.test(f.text)));
    assert.equal(Number.isInteger(scan.rows), true);
});
