-- The SQL performance course's example schema: a small shop, generated deterministically (setseed), so every plan the
-- course shows can be captured again and compared. Loaded by tools/sqlperf/capture.py into a throwaway database only.
--
--   customers    50,000      one row per customer, with a country and a signup date
--   products     10,000      a catalogue with categories and prices
--   orders    2,000,000 × scale  customer, status, total, created_at spread over 600 days
--   order_items  ~6,000,000  one to five lines per order
--
-- Indexes beyond the primary keys are created by each plan's setup in content/sql/setups.toml, never here, so a page
-- states exactly which indexes its plan ran with.

-- psql -v scale=N multiplies the order and item counts (customers and products stay fixed); the default is 1.
\if :{?scale}
\else
\set scale 1
\endif

SET client_min_messages = warning;
SELECT setseed(0.42);

DROP TABLE IF EXISTS order_items, orders, products, customers CASCADE;

CREATE TABLE customers (
    id         int         PRIMARY KEY,
    email      text        NOT NULL,
    name       text        NOT NULL,
    country    char(2)     NOT NULL,
    created_at timestamptz NOT NULL
);

CREATE TABLE products (
    id          int     PRIMARY KEY,
    sku         text    NOT NULL,
    name        text    NOT NULL,
    category    text    NOT NULL,
    price_cents int     NOT NULL
);

CREATE TABLE orders (
    id          bigint      GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    customer_id int         NOT NULL REFERENCES customers (id),
    status      text        NOT NULL,
    total_cents int         NOT NULL,
    created_at  timestamptz NOT NULL
);

CREATE TABLE order_items (
    order_id    bigint NOT NULL REFERENCES orders (id),
    line        int    NOT NULL,
    product_id  int    NOT NULL REFERENCES products (id),
    quantity    int    NOT NULL,
    price_cents int    NOT NULL,
    PRIMARY KEY (order_id, line)
);

INSERT INTO customers (id, email, name, country, created_at)
SELECT i,
       'user' || i || '@example.com',
       'Customer ' || i,
       (ARRAY['US', 'US', 'US', 'IN', 'IN', 'GB', 'DE', 'FR', 'BR', 'JP'])[1 + (random() * 9)::int],
       timestamptz '2023-01-01' + random() * interval '1000 days'
FROM generate_series(1, 50000) AS i;

INSERT INTO products (id, sku, name, category, price_cents)
SELECT i,
       'SKU-' || lpad(i::text, 6, '0'),
       'Product ' || i,
       (ARRAY['books', 'games', 'garden', 'kitchen', 'music', 'outdoor', 'tools', 'toys'])[1 + (random() * 7)::int],
       100 + (random() * 19900)::int
FROM generate_series(1, 10000) AS i;

-- statuses are skewed on purpose (most orders are paid), so estimates and selectivity have something to show
INSERT INTO orders (customer_id, status, total_cents, created_at)
SELECT 1 + (random() * 49999)::int,
       (ARRAY['paid', 'paid', 'paid', 'paid', 'paid', 'paid', 'shipped', 'shipped', 'refunded', 'pending'])[1 + (random() * 9)::int],
       (random() * 20000)::int,
       timestamptz '2025-01-01' + random() * interval '600 days'
FROM generate_series(1, 2000000 * :scale);

INSERT INTO order_items (order_id, line, product_id, quantity, price_cents)
SELECT o.id, l, 1 + (random() * 9999)::int, 1 + (random() * 3)::int, 100 + (random() * 19900)::int
FROM orders o, generate_series(1, 1 + (o.id % 5)::int) AS l;

VACUUM ANALYZE customers, products, orders, order_items;
