"""Runs a Python practice problem's tests. Started as `python3 _harness.py visible [hidden]` in the work directory,
which holds the user's solution.py, the test modules and anneal_prelude.py. Prints one `@@anneal@@<json>` line per event;
anything else on stdout (the user's prints) is captured per test and reported with it."""
import contextlib
import io
import json
import os
import re
import signal
import sys
import time
import traceback

REAL_STDOUT = sys.__stdout__
PER_TEST_SECONDS = float(os.environ.get("ANNEAL_TEST_SECONDS", "4"))
OUTPUT_LIMIT = 16 * 1024


def emit(event):
    REAL_STDOUT.write("@@anneal@@" + json.dumps(event) + "\n")
    REAL_STDOUT.flush()


def cut(text, limit):
    """The first `limit` characters, without half an escape sequence at the end, and with the colour reset."""
    text = re.sub(r"\x1b(\[[0-?]*[ -/]*|[\]PX^_][^\x07\x1b\n]*|[ -/]*)$", "", text[:limit])
    return text + "\x1b[0m" if "\x1b" in text else text


class TestTimeout(BaseException):
    pass


armed = [False]


def on_alarm(signum, frame):
    if armed[0]:
        armed[0] = False
        raise TestTimeout()


def solution_line(tb):
    """The innermost line of the traceback that is in solution.py, or None."""
    line = None
    for frame in traceback.extract_tb(tb):
        if os.path.basename(frame.filename) == "solution.py":
            line = frame.lineno
    return line


def report_error(kind, exc):
    line = solution_line(exc.__traceback__)
    emit({"kind": kind, "message": f"{type(exc).__name__}: {exc}", "line": line or 1, "col": 1,
          "trace": "".join(format_trace(exc)[-6:])})


def format_trace(exc):
    """The traceback's lines, in the interpreter's own colours where it has them (Python 3.13 and later)."""
    try:
        return list(traceback.TracebackException.from_exception(exc).format(colorize=True))
    except TypeError:
        return traceback.format_exception(type(exc), exc, exc.__traceback__)


def main(suites):
    try:
        with open("solution.py", encoding="utf-8") as f:
            source = f.read()
        compile(source, "solution.py", "exec")
    except SyntaxError as e:
        emit({"kind": "syntax", "message": e.msg, "line": e.lineno or 1, "col": e.offset or 1, "text": (e.text or "").rstrip("\n")})
        return
    try:
        import solution  # noqa: F401  (tests import from it)
    except BaseException as e:  # runs the module's top-level code
        report_error("import_error", e)
        return

    import anneal_prelude

    signal.signal(signal.SIGALRM, on_alarm)
    for suite in suites:
        try:
            module = __import__(suite)
        except ImportError as e:
            # `from solution import name` for a name the user hasn't defined (yet).
            emit({"kind": "missing", "message": str(e), "name": getattr(e, "name", None) or "", "line": 1})
            return
        except BaseException as e:
            report_error("test_file_error", e)
            return
        for name, fn in list(vars(module).items()):
            if not (name.startswith("test_") and callable(fn)):
                continue
            buf = io.StringIO()
            outcome, check, panic = "passed", None, None
            started = time.perf_counter()
            try:
                try:
                    armed[0] = True
                    signal.setitimer(signal.ITIMER_REAL, PER_TEST_SECONDS)
                    with contextlib.redirect_stdout(buf):
                        fn()
                except TestTimeout:
                    outcome, panic = "timed_out", f"took longer than {PER_TEST_SECONDS:g} s"
                except anneal_prelude.Mismatch as m:
                    outcome = "failed"
                    check = {"input": m.call, "expected": m.expected, "got": m.got}
                    line = solution_line(m.__traceback__)
                    panic = str(m) if line is None else f"{m}\n(solution.py line {line})"
                except BaseException as e:  # assertion, exception in the user's code, RecursionError ...
                    outcome = "failed"
                    line = solution_line(e.__traceback__)
                    text = str(e)
                    panic = f"{type(e).__name__}: {text}" if text else type(e).__name__
                    if line is not None:
                        panic += f"\n(solution.py line {line})"
                finally:
                    # From here on a late alarm is ignored. One that lands just before this line (the test finished in the
                    # same instant it ran out of time) is caught below and reported as a timeout.
                    armed[0] = False
                    signal.setitimer(signal.ITIMER_REAL, 0)
            except TestTimeout:
                outcome, panic, check = "timed_out", f"took longer than {PER_TEST_SECONDS:g} s", None
            out = buf.getvalue()
            if len(out) > OUTPUT_LIMIT:
                out = cut(out, OUTPUT_LIMIT) + "\n… output cut"
            emit({"kind": "test", "suite": suite, "name": name[len("test_"):], "outcome": outcome,
                  "ms": (time.perf_counter() - started) * 1000.0, "check": check, "panic": panic, "stdout": out})
    emit({"kind": "done"})


if __name__ == "__main__":
    sys.setrecursionlimit(3000)
    main(sys.argv[1:])
