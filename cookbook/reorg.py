#!/usr/bin/env python3
"""One-time (idempotent) move of flat examples/*.rs into chapter folders."""
import os, shutil
from layout import parse, folder_for, all_files
moved = 0
for f in all_files():
    if f.count("/") != 1: continue  # already in a folder
    d = parse(f); folder = folder_for(d)
    os.makedirs(f"examples/{folder}", exist_ok=True)
    shutil.move(f, f"examples/{folder}/{d['file']}"); moved += 1
    if folder == "misc": print("UNMAPPED:", d["file"], d["cat"])
print("moved", moved)
