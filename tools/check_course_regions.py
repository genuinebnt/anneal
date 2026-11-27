#!/usr/bin/env python3
"""Every `// @begin`, `// @end` and `//~` stub line of a course reference must start its own line.

The renderer (crates/cli/src/course.rs, render_text) only recognises them at the start of a line. rustfmt can pull a comment that
follows the last block arm of a `match` onto the line of its closing brace (`} //~ ...`), which silently drops that stub from the
learner's code. Keep the region's stub before an arm that follows it, or end the region with a comma-terminated arm.
"""
import glob
import re
import sys

MARK = re.compile(r"(//~|// @begin\b|// @end\b|#~|# @begin\b|# @end\b)")
bad = []
for root in sys.argv[1:] or ["courses"]:
    for path in glob.glob(f"{root}/*/reference/**/*.rs", recursive=True) + glob.glob(f"{root}/*/reference/**/Cargo.toml", recursive=True):
        if "/target/" in path:
            continue
        for n, line in enumerate(open(path, encoding="utf-8"), 1):
            m = MARK.search(line)
            if not m or line[: m.start()].strip() == "":
                continue
            before = line[: m.start()]
            if before.count('"') % 2 == 1:  # inside a string literal
                continue
            bad.append(f"{path}:{n}: {line.strip()[:120]}")
for b in bad:
    print(b)
print(f"{len(bad)} region markers or stub lines that do not start their line")
sys.exit(1 if bad else 0)
