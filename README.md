# anneal

A personal Rust interview-prep platform for SDE-2 / SDE-3 backend roles: DSA, Rust depth, and
build-it tracks, each running easy → hard, with a sandboxed runner and an in-browser workspace.

- Plan and decisions: [docs/PLAN.md](docs/PLAN.md)
- Curriculum: [docs/CURRICULUM.md](docs/CURRICULUM.md)
- Designs: [docs/design_handoff_anneal/designs/anneal-screens.html](docs/design_handoff_anneal/designs/anneal-screens.html)
  (opens directly in a browser; the original 1a–1d designs need `./serve.sh` in that folder)

## Run it

Needs Rust with the `rust-analyzer` and `rust-src` components (`rustup component add rust-analyzer rust-src`),
pnpm, and OrbStack (the sandbox is pinned to the `orbstack` Docker context).

```sh
docker compose up -d                                                    # Postgres on :5435
docker build -t anneal-runner:1.98 -f docker/runner.Dockerfile docker   # sandbox image (rebuild after changing docker/deps)
(cd web && pnpm install && pnpm build)
cargo run -p anneal-api                                                 # http://localhost:8787
```

**Day to day, use dev mode:** `./scripts/dev.sh`, then open the web URL it prints (normally
http://127.0.0.1:5180; if :8787 or :5180 is taken by another program it picks a random free port and says so).
**`./scripts/restart.sh`** stops a running dev server, runs `git pull --rebase --autostash`, and starts dev mode
again. The API restarts whenever
`crates/`, `content/` or the Cargo files change, and the web app hot-reloads through Vite (which proxies `/api`
to the server). A `git pull` is picked up the same way. The script also points git at `.githooks`, whose
post-merge/post-rewrite hooks run `pnpm install` when web dependencies change and warn when `docker/` changed
(rebuild the runner image then).

The server reads its settings from the environment; `cargo run` fills in local defaults from
`.cargo/config.toml`. See the table at the top of [crates/api/src/main.rs](crates/api/src/main.rs).

### Login

Without `ANNEAL_PASSPHRASE_HASH` there's no login, and the server only starts on a loopback address.
To require a passphrase (and before listening anywhere else):

```sh
cargo run -p anneal-cli -- passphrase        # prompts twice, prints ANNEAL_PASSPHRASE_HASH='$argon2id$…'
export ANNEAL_PASSPHRASE_HASH='$argon2id$…'  # single quotes: the hash contains $
export ANNEAL_COOKIE_SECURE=true             # when served over HTTPS
cargo run -p anneal-api
```

Sessions last 30 days in an HttpOnly, SameSite=Strict cookie; only a SHA-256 of each token is stored. Five wrong
passphrases in a row pause logins for a minute. Editor settings (the **Aa** button above the editor: font size, font family, Vim mode with `jk`/`kj` to leave insert mode) are saved
on the server.

### Crates in problems

A problem may use crates from the fixed set in [docker/deps/Cargo.toml](docker/deps/Cargo.toml) by listing them in
`problem.toml`: `crates = ["tokio", "serde"]`. The runner adds those dependency lines (versions pinned by
`docker/deps/Cargo.lock`). In the Docker sandbox they come from the copy vendored into the image, so runs stay
offline; host runs fetch from crates.io. To change the set, edit the manifest, run `cargo generate-lockfile` in
`docker/deps`, and rebuild the image.

## Production

https://anneal.genuinebasil.dev, deployed from `master` by CI on every green push. See
[docs/DEPLOY.md](docs/DEPLOY.md).

## Content

Problems live in `content/tracks/<track>/problems/<slug>/` as a `problem.toml`, `statement.md`,
`starter.rs`, `solution.rs` and `tests/{visible,hidden}.rs`. Tests use `check!(input, got, expected)`
so failures show input / expected / got.

```sh
cargo run -p anneal-cli -- validate                  # check every track and problem
cargo run -p anneal-cli -- list d9                   # problems in a track
cargo run -p anneal-cli -- run d9-network-delay-time --solution --submit --docker
```

## Cookbook

A reference section at `/cookbook`, loaded from `content/cookbook`; it stores nothing about the reader. How it works, how
to regenerate its guide pages and how to add a page: [docs/COOKBOOK.md](docs/COOKBOOK.md).

## Tests

```sh
cargo test --workspace                               # needs the compose Postgres
cargo test -p anneal-runner -- --ignored             # the Docker sandbox test
```
