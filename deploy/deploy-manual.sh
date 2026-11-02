#!/usr/bin/env bash
# Builds master on this box and deploys it without GitHub Actions or the registry. Run it on the VM, in the checkout
# (~/anneal), as the deploy user; nothing is pushed anywhere and no secret is printed.
#
#   deploy/deploy-manual.sh               deploy origin/master
#   deploy/deploy-manual.sh <git-sha>     deploy that commit
#
# It builds the images with the same Dockerfiles and tags CI uses, then hands over to deploy/deploy.sh, which backs up
# the database, runs the progress-safety preflight, restarts the stack and fails (leaving the backup) if the new
# version never becomes healthy. To go back, run `deploy/deploy.sh <previous sha>` while its images are still here.
set -euo pipefail
cd "$(dirname "$0")/.."

app=ghcr.io/genuinebnt/anneal
runner=ghcr.io/genuinebnt/anneal-runner
log() { echo "deploy-manual: $*"; }

build_images() {
  local sha=$1 label="org.opencontainers.image.revision=$1"
  # The app image serves the CLI installers from dist-cli/. CI builds the macOS ones; here they are carried over from
  # the release that is running now, so /install.sh keeps working.
  mkdir -p dist-cli
  if docker inspect anneal-app >/dev/null 2>&1; then
    docker cp anneal-app:/app/downloads/. dist-cli/ 2>/dev/null || true
  fi

  log "building the app image (the first build on a fresh box takes a while)"
  docker build -q -t "$app:$sha" --label "$label" . >/dev/null

  # courses/bustub/solutions.json is generated from the reference and is not in git: make it with the CLI just built,
  # then rebuild, which only redoes the cheap COPY layers.
  log "generating the solutions the app serves"
  # The reference is in the checkout but kept out of the image (.dockerignore), so run against the checkout.
  docker run --rm --user "$(id -u):$(id -g)" -v "$PWD:/repo" -w /repo "$app:$sha" \
    anneal course solutions --out courses/bustub/solutions.json
  # The fresh Linux CLI replaces the carried-over one, so the downloads match this commit.
  docker run --rm --entrypoint cat "$app:$sha" /usr/local/bin/anneal > dist-cli/anneal-linux-x64
  chmod +x dist-cli/anneal-linux-x64
  docker build -q -t "$app:$sha" --label "$label" . >/dev/null

  log "building the runner image"
  docker build -q -t "$runner:$sha" --label "$label" -f docker/runner.Dockerfile docker >/dev/null
}

# Each build leaves a large layer cache; after a deploy that worked keep only the newest slice (BUILD_CACHE_KEEP,
# default 4GB). Images are untouched, and a failure here never fails the deploy.
trim_build_cache() {
  if docker builder prune -f --keep-storage "${BUILD_CACHE_KEEP:-4GB}" >/dev/null 2>&1; then
    log "build cache trimmed: $(df -h / | awk 'NR==2 {print $4}') free"
  else
    log "could not trim the build cache (ignored)"
  fi
}

# Everything is inside main so bash has parsed the whole script before `git reset` rewrites this file.
main() {
  git fetch -q origin master
  if [ -z "${1:-}" ] || [ "${1:-}" = master ]; then sha=$(git rev-parse origin/master); else sha=$(git rev-parse "$1^{commit}"); fi
  git reset -q --hard "$sha"
  build_images "$sha"
  ANNEAL_SKIP_PULL=1 deploy/deploy.sh "$sha"
  log "live: ${sha:0:12}"
  trim_build_cache
}

main "$@"
exit
