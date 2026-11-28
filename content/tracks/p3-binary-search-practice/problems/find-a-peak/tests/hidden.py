import random

from anneal_prelude import check, ensure
from solution import find_a_peak

def is_peak(values, i):
    left = values[i - 1] if i > 0 else float("-inf")
    right = values[i + 1] if i + 1 < len(values) else float("-inf")
    return values[i] > left and values[i] > right


def expect_peak(values):
    i = find_a_peak(list(values))
    ensure(isinstance(i, int) and 0 <= i < len(values), f"find_a_peak({values}) returned {i}, which is not an index")
    ensure(is_peak(values, i), f"find_a_peak({values}) returned {i}, but {values[i]} is not greater than both neighbours")


def test_two_values():
    expect_peak([1, 2])
    expect_peak([2, 1])


def test_a_single_value_is_a_peak():
    check("find_a_peak([9])", find_a_peak([9]), 0)


def test_a_zig_zag():
    expect_peak([1, 3, 1, 3, 1, 3, 1])


def test_every_arrangement_of_five_distinct_numbers():
    import itertools

    for p in itertools.permutations(range(5)):
        expect_peak(list(p))


def test_random_lists_with_no_equal_neighbours():
    rng = random.Random(45)
    for _ in range(300):
        n = rng.randint(1, 15)
        values = [rng.randint(0, 30)]
        while len(values) < n:
            v = rng.randint(0, 30)
            if v != values[-1]:
                values.append(v)
        expect_peak(values)


def test_a_million_values_is_fast():
    values = list(range(500_000)) + list(range(500_000, 0, -1))
    expect_peak(values)


def test_it_does_not_read_every_value():
    class Counted(list):
        reads = 0

        def __getitem__(self, i):
            Counted.reads += 1
            return super().__getitem__(i)

    values = Counted(list(range(1_000_000, 0, -1)))
    find_a_peak(values)
    ensure(Counted.reads < 200, f"{Counted.reads} values were read; binary search needs about 40")


def test_a_list_too_big_to_scan():
    class Hill:
        """10**12 values rising to a peak at index 700_000_000_000 and falling after it."""

        def __len__(self):
            return 10**12

        def __getitem__(self, i):
            if i < 0:
                i += 10**12  # negative indexes count from the end, like a list
            if not 0 <= i < 10**12:
                raise IndexError(i)
            return -abs(i - 700_000_000_000)

    check("find_a_peak(a hill of 10**12 values)", find_a_peak(Hill()), 700_000_000_000)


def test_a_peak_at_the_last_index_of_a_long_climb():
    values = list(range(100_000))
    check("find_a_peak(a long climb)", find_a_peak(values), 99_999)
