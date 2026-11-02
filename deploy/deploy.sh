#!/usr/bin/env bash
# Deploys anneal on the production server. CI runs it over SSH after pushing the images for a commit on master;
# the deploy key is restricted to this script (see deploy/ssh-deploy.sh and docs/DEPLOY.md).
#
#   deploy/deploy.sh <git-sha>     deploy that commit's images
#   deploy/deploy.sh               redeploy the latest images
#
# A registry token for ghcr.io may arrive on stdin (CI passes its short-lived GITHUB_TOKEN); it's used for the pull
# and then logged out, so nothing long-lived is stored on the server.
set -euo pipefail
cd "$(dirname "$0")/.."

# Everything is inside main so bash has parsed the whole script before `git reset` below can rewrite this file.
main() {
  tag=${1:-latest}
  if ! [[ $tag =~ ^([0-9a-f]{7,40}|latest)$ ]]; then
    echo "deploy: bad tag '$tag'" >&2
    exit 2
  fi

  log() { echo "deploy: $*"; }
  compose() { ANNEAL_TAG=$tag docker compose -f deploy/compose.prod.yml --env-file deploy/.env "$@"; }

  # The compose file and this script come from the checkout, so bring it to the deployed commit.
  log "updating the checkout to ${tag}"
  git fetch -q origin master
  if [ "$tag" = latest ]; then git reset -q --hard origin/master; else git reset -q --hard "$tag"; fi

  token=""
  if [ ! -t 0 ]; then token=$(cat); fi
  if [ -n "$token" ]; then
    echo "$token" | docker login ghcr.io -u genuinebnt --password-stdin >/dev/null
    trap 'docker logout ghcr.io >/dev/null 2>&1 || true' EXIT
  fi

  # deploy/deploy-manual.sh builds the images on this box and sets ANNEAL_SKIP_PULL, so there is nothing to pull.
  if [ -z "${ANNEAL_SKIP_PULL:-}" ]; then
    log "pulling images"
    compose pull --quiet
    # The API starts the runner image itself; pull it now so the first run after a deploy doesn't wait on it.
    docker pull --quiet "ghcr.io/genuinebnt/anneal-runner:${tag}" >/dev/null
  fi

  # Progress safety: a backup first, then the new version checks the live database (stranded progress, edited
  # migrations) before anything is replaced. If preflight fails, the running version keeps serving.
  compose up -d --wait postgres
  deploy/backup.sh "pre-${tag:0:12}"
  log "preflight"
  compose run --rm --no-deps -T app anneal-api preflight

  log "starting"
  compose up -d --remove-orphans --wait --wait-timeout 180

  # Keep the disk clean: old anneal images from previous deploys, and dangling layers.
  docker image prune -f >/dev/null
  for repo in ghcr.io/genuinebnt/anneal ghcr.io/genuinebnt/anneal-runner; do
    docker images "$repo" --format '{{.Tag}} {{.ID}}' | awk -v keep="$tag" '$1 != keep && $1 != "latest" {print $2}' |
      xargs -r docker rmi >/dev/null 2>&1 || true
  done

  log "done: $(compose ps --format '{{.Service}} {{.Status}}' | tr '\n' ' ')"
}

main "$@"
exit
