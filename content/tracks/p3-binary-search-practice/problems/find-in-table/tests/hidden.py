import random

from anneal_prelude import check, ensure
from solution import find_in_table


def test_single_cell_found():
    check('find_in_table([[5]], 5)', find_in_table([[5]], 5), (0, 0))


def test_single_cell_missing():
    check('find_in_table([[5]], 6)', find_in_table([[5]], 6), None)


def test_one_column():
    check('find_in_table([[1], [3], [5], [7]], 5)', find_in_table([[1], [3], [5], [7]], 5), (2, 0))


def test_one_row():
    check('find_in_table([[1, 3, 5, 7]], 7)', find_in_table([[1, 3, 5, 7]], 7), (0, 3))


def test_below_everything():
    check('find_in_table([[10, 20], [30, 40]], 1)', find_in_table([[10, 20], [30, 40]], 1), None)


def test_above_everything():
    check('find_in_table([[10, 20], [30, 40]], 99)', find_in_table([[10, 20], [30, 40]], 99), None)


def test_every_cell_of_random_tables_is_found():
    rng = random.Random(43)
    for _ in range(100):
        rows, cols = rng.randint(1, 6), rng.randint(1, 6)
        values = sorted(rng.sample(range(0, 200), rows * cols))
        table = [values[r * cols:(r + 1) * cols] for r in range(rows)]
        for r in range(rows):
            for c in range(cols):
                check(f"find_in_table({table}, {table[r][c]})", find_in_table(table, table[r][c]), (r, c))
        absent = [v for v in range(-1, 201) if v not in values][:5]
        for v in absent:
            check(f"find_in_table({table}, {v})", find_in_table(table, v), None)


def test_a_table_too_big_to_scan():
    class Rows:
        """A table of 10**6 rows of 10**6 columns, built row by row on request."""

        def __len__(self):
            return 10**6

        def __getitem__(self, r):
            if r < 0:
                r += 10**6  # negative indexes count from the end, like a list
            if not 0 <= r < 10**6:
                raise IndexError(r)
            return range(r * 10**6, (r + 1) * 10**6)

    check("find_in_table(10**6 x 10**6, 654_321_987_654)", find_in_table(Rows(), 654_321_987_654), (654_321, 987_654))
    check("find_in_table(10**6 x 10**6, -3)", find_in_table(Rows(), -3), None)


def test_a_table_of_a_million_cells_is_fast():
    table = [list(range(r * 1000, r * 1000 + 1000)) for r in range(1000)]
    check("find_in_table(1000 x 1000, 654321)", find_in_table(table, 654321), (654, 321))
    check("find_in_table(1000 x 1000, -1)", find_in_table(table, -1), None)
