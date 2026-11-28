import random

from anneal_prelude import check, ensure
from solution import fewest_bills


def test_count_of_zero_is_ignored():
    check('fewest_bills([(5, 0), (1, 9)], 5)', fewest_bills([(5, 0), (1, 9)], 5), 5)


def test_exact_with_a_big_bill():
    check('fewest_bills([(100, 1), (1, 50)], 100)', fewest_bills([(100, 1), (1, 50)], 100), 1)


def test_two_big_bills():
    check('fewest_bills([(50, 2), (20, 5)], 100)', fewest_bills([(50, 2), (20, 5)], 100), 2)


def test_the_limit_makes_it_impossible():
    check('fewest_bills([(25, 1), (10, 5)], 60)', fewest_bills([(25, 1), (10, 5)], 60), -1)


def test_unlimited_would_be_cheaper():
    check('fewest_bills([(7, 1), (3, 4)], 12)', fewest_bills([(7, 1), (3, 4)], 12), 4)


def test_impossible_remainder():
    check('fewest_bills([(5, 5), (10, 5)], 12)', fewest_bills([(5, 5), (10, 5)], 12), -1)


def reference(bills, amount):
    best = float("inf")

    def go(i, left, used):
        nonlocal best
        if left == 0:
            best = min(best, used)
            return
        if i == len(bills):
            return
        value, count = bills[i]
        for k in range(count + 1):
            if k * value > left:
                break
            go(i + 1, left - k * value, used + k)

    go(0, amount, 0)
    return -1 if best == float("inf") else best


def test_random_tills_against_trying_every_combination():
    rng = random.Random(14)
    for _ in range(300):
        bills = [(rng.randint(1, 8), rng.randint(0, 4)) for _ in range(rng.randint(0, 4))]
        amount = rng.randint(0, 25)
        check(f"fewest_bills({bills}, {amount})", fewest_bills(bills, amount), reference(bills, amount))


def test_a_bigger_till_is_fast_enough():
    bills = [(1, 20), (2, 20), (5, 20), (10, 20), (20, 20), (50, 20)]
    got = fewest_bills(bills, 997)
    ensure(got > 0, "997 can be made from these bills")
    check("fewest_bills([(1, 20), (2, 20), (5, 20), (10, 20), (20, 20), (50, 20)], 997)", got, 23)
