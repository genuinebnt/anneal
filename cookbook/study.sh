#!/usr/bin/env bash
# study.sh NAME   : make a private scratch copy of an example to experiment with (does not touch the original)
#   ./study.sh lifetimes          -> examples/scratch/lifetimes_mine.rs  (run: cargo run --release --example lifetimes_mine)
#   ./study.sh lifetimes --blank  -> same, but with every `assert!`/`assert_eq!` kept and function bodies left for YOU to rewrite:
#                                    you get the header (the spec) and main() (the tests); delete a function body and make the asserts pass again.
cd "$(dirname "$0")" || exit 1
name="$1"; src=$(ls examples/*/"$name".rs 2>/dev/null | head -1)
[ -z "$src" ] && { echo "no example named '$name' (try: ls examples/*/ | grep $name)"; exit 1; }
mkdir -p examples/scratch
dst="examples/scratch/${name}_mine.rs"
cp "$src" "$dst"
# make the scratch copy self-describing and keep it out of the index/check
sed -i.bak "1s|^|//! SCRATCH COPY of $src: edit freely. The original is untouched.\n|" "$dst" && rm -f "$dst.bak"
python3 gen_manifest.py >/dev/null
echo "created $dst"
echo "run it:  cargo run --release --example ${name}_mine"
echo "study loop: predict the output -> run -> break something on purpose -> predict the error -> fix -> rewrite one function from memory"
