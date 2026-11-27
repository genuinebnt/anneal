"""The reference server, cut by capstone step from template.rs.

Marker lines open a block that `//@ end` closes (blocks do not nest):
  //@ from N     in the solution of step N and later, and so in the starters after N
  //@ only A B   as `from A`, but gone after step B
  //@ given N    as `from N`, and also in the starter of step N
  //@ stub N     only in the starter of step N
"""
import pathlib

TEMPLATE = (pathlib.Path(__file__).parent / "template.rs").read_text()


def _cut(keep):
    out, block = [], None
    for n, line in enumerate(TEMPLATE.splitlines(), 1):
        mark = line.strip()
        if mark.startswith("//@"):
            words = mark[3:].split()
            if words == ["end"]:
                assert block is not None, f"line {n}: stray end"
                block = None
            else:
                assert block is None, f"line {n}: nested block"
                block = (words[0], [int(w) for w in words[1:]])
            continue
        if block is None or keep(*block):
            out.append(line)
    assert block is None
    # blank lines left where blocks went are collapsed
    text = []
    for line in out:
        if line.strip() or (text and text[-1].strip()):
            text.append(line)
    return "\n".join(text).strip("\n") + "\n"


def solution(step):
    def keep(kind, n):
        if kind in ("from", "given"):
            return step >= n[0]
        if kind == "only":
            return n[0] <= step <= n[1]
        return False

    return _cut(keep)


def starter(step):
    below = step - 1

    def keep(kind, n):
        if kind == "from":
            return below >= n[0]
        if kind == "given":
            return step >= n[0]
        if kind == "only":
            return n[0] <= below <= n[1]
        return n[0] == step

    return _cut(keep)
