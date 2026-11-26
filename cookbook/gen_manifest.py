#!/usr/bin/env python3
"""Regenerate the [[example]] entries in Cargo.toml from the files under examples/<chapter>/*.rs.
Run after adding, moving or removing an example:   python3 gen_manifest.py"""
import glob, os
BEGIN, END = "# BEGIN GENERATED EXAMPLES (python3 gen_manifest.py)", "# END GENERATED EXAMPLES"
s = open("Cargo.toml").read()
if BEGIN in s:
    s = s[: s.index(BEGIN)].rstrip("\n") + "\n"
block = [BEGIN]
for f in sorted(glob.glob("examples/*/*.rs")):
    block += ["[[example]]", f'name = "{os.path.basename(f)[:-3]}"', f'path = "{f}"']
block.append(END)
open("Cargo.toml", "w").write(s.rstrip("\n") + "\n\n" + "\n".join(block) + "\n")
print(f"{len(block) // 3} examples registered")
