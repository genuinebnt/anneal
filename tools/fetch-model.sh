#!/usr/bin/env bash
# Downloads the meaning model (bge-small-en-v1.5, pinned to one revision) and checks every file against its SHA-256.
#   tools/fetch-model.sh [dir]     default: ~/.cache/anneal/models/bge-small-en-v1.5
# Then run the API with ANNEAL_MODEL=<dir>. The Dockerfile pins the same files with ADD --checksum; keep the two in step.
set -euo pipefail
dir="${1:-$HOME/.cache/anneal/models/bge-small-en-v1.5}"
rev=5c38ec7c405ec4b44b94cc5a9bb96e735b38267a
mkdir -p "$dir"
while read -r sum file; do
    if [ -f "$dir/$file" ] && echo "$sum  $dir/$file" | shasum -a 256 -c --status 2>/dev/null; then continue; fi
    curl -fsSL --retry 3 -o "$dir/$file.part" "https://huggingface.co/BAAI/bge-small-en-v1.5/resolve/$rev/$file"
    echo "$sum  $dir/$file.part" | shasum -a 256 -c --status || { echo "checksum mismatch: $file" >&2; rm -f "$dir/$file.part"; exit 1; }
    mv "$dir/$file.part" "$dir/$file"
done <<'SUMS'
094f8e891b932f2000c92cfc663bac4c62069f5d8af5b5278c4306aef3084750 config.json
d241a60d5e8f04cc1b2b3e9ef7a4921b27bf526d9f6050ab90f9267a1f9e5c66 tokenizer.json
3c9f31665447c8911517620762200d2245a2518d6e7208acc78cd9db317e21ad model.safetensors
SUMS
echo "$dir"
