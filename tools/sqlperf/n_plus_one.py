"""Times N single-row queries against one `= ANY($1)` query, on a local Postgres (module 14 of the SQL course).

Run: python3 tools/sqlperf/n_plus_one.py   (needs psycopg; the example schema on port 5418; creates and drops one index)."""
import psycopg, time, statistics
c = psycopg.connect("postgresql://anneal@127.0.0.1:5418/sqlperf", autocommit=True)
cur = c.cursor()
cur.execute("CREATE INDEX IF NOT EXISTS orders_customer ON orders (customer_id)"); cur.execute("ANALYZE orders")
def t(f, n=7):
    xs=[]
    for _ in range(n):
        s=time.perf_counter(); f(); xs.append((time.perf_counter()-s)*1000)
    return statistics.median(xs)
for N in (20, 100, 500):
    ids=list(range(1000, 1000+N))
    def loop():
        for i in ids:
            cur.execute("SELECT id, total_cents FROM orders WHERE customer_id = %s", (i,)); cur.fetchall()
    def loop_prep():
        for i in ids:
            cur.execute("SELECT id, total_cents FROM orders WHERE customer_id = %s", (i,), prepare=True); cur.fetchall()
    def any_():
        cur.execute("SELECT id, customer_id, total_cents FROM orders WHERE customer_id = ANY(%s)", (ids,)); cur.fetchall()
    rows=len(cur.execute("SELECT id FROM orders WHERE customer_id = ANY(%s)", (ids,)).fetchall())
    print(N, "rows", rows, "loop %.1f ms" % t(loop), "loop(prepared) %.1f ms" % t(loop_prep), "ANY %.1f ms" % t(any_))
cur.execute("DROP INDEX orders_customer")
