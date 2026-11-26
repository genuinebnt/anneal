# Deploying a Rust service: a production guide

Direct how-to for the infrastructure around services like the ones in this cookbook (axum + tokio, optionally
Postgres). This is boilerplate and operations, so it is explained plainly.

**What has been checked.** Every link returned HTTP 200 when this was written. The facts about Cloud Run shutdown,
Secret Manager environment variables, the Cloud SQL socket path, distroless tags, the Kubernetes default grace
period and `available_parallelism` were read on the linked pages. The shutdown code in section 5 was compiled.
**The Dockerfile was not built, the YAML was not applied, and the Terraform was not run or validated.** Treat those
as starting points. `my-service`, `PROJECT` and `REGION` are placeholders.

## 1. Release profile

```toml
[profile.release]
opt-level = 3
lto = "thin"          # "fat" is slower to link; measure before paying for it
codegen-units = 1     # better optimisation, slower builds
debug = "line-tables-only"   # file and line in backtraces and profiles
strip = "none"        # see below
overflow-checks = true       # default is false in release: wrapping arithmetic is a security and data bug
panic = "unwind"      # see below
```

- `panic = "unwind"` versus `"abort"`: with unwind, tokio catches a panic at the task boundary, so one bad request
  fails and the process lives. With abort, any panic kills the process and every in-flight request. Abort gives a
  somewhat smaller binary and a fail-fast restart. For a request server keep unwind, count panics, and alert on them.
  A panic while holding a `std::sync::Mutex` poisons it; decide what your code does then.
- Stripping: for readable crash reports either keep line tables in the binary, or strip and upload the separate
  debug file to your error tracker. `strip = true` removes symbols, so backtraces become addresses.
- `target-cpu=native` produces a binary that may crash with an illegal instruction on a different machine. Pick an
  explicit baseline that every production node supports.
- Measure each setting on your own workload. Reference: <https://doc.rust-lang.org/cargo/reference/profiles.html>

## 2. Reproducible builds

1. Commit `Cargo.lock`. Build with `cargo build --release --locked`, which fails if the lock file would change
   (<https://doc.rust-lang.org/cargo/commands/cargo-build.html>).
2. Pin the toolchain in `rust-toolchain.toml` (<https://rust-lang.github.io/rustup/overrides.html>).
3. Pin base images by digest (`image@sha256:...`), not only by tag. Refresh digests on a schedule so security
   updates still arrive.
4. Remove machine-specific input: build in a fixed directory (or use `--remap-path-prefix`), do not embed the wall
   clock. `SOURCE_DATE_EPOCH` is the convention for tools that need a timestamp
   (<https://reproducible-builds.org/docs/source-date-epoch/>).
5. Verify it: build twice on clean machines and compare hashes. Until that passes, do not claim reproducibility.
6. In CI also run `cargo audit` or `cargo deny` against the lock file.

## 3. Docker: multi-stage, distroless or musl

<https://docs.docker.com/build/building/multi-stage/> and <https://github.com/GoogleContainerTools/distroless>

```dockerfile
# syntax=docker/dockerfile:1
# The builder's Debian release must match the runtime's, or the binary may need a newer glibc than the runtime has.
FROM rust:1-trixie AS build
WORKDIR /src
COPY . .
RUN cargo build --release --locked --bin my-service

FROM gcr.io/distroless/cc-debian13:nonroot
COPY --from=build /src/target/release/my-service /my-service
ENTRYPOINT ["/my-service"]
```

Pitfalls:

- **`.dockerignore`** must exclude `target/`, `.git/`, `.env` and key files. `COPY . .` copies whatever you forget.
- **Dependency caching.** The simple file above recompiles all dependencies on any source change. Use BuildKit cache
  mounts for the cargo registry and `target/` (copy the binary out within the same `RUN`, because a cache mount is
  not part of the layer), or use `cargo-chef`.
- **Exec form** (`["/my-service"]`) makes your binary PID 1 and the receiver of SIGTERM. The shell form puts
  `/bin/sh` in front (distroless has no shell at all). As PID 1, a signal with no handler is ignored by default, so
  install the handler in section 5.
- **`cc` or `static`.** `cc-debian13` has glibc and libgcc. `static-debian13` is for fully static binaries, for
  example built for `x86_64-unknown-linux-musl`.
- **musl** gives a static binary, but crates with C code (for example `rusqlite` with `bundled`, or TLS stacks) need a
  musl C toolchain, and musl's allocator can be much slower under multithreaded allocation. Benchmark, and consider
  another global allocator. Target tiers: <https://doc.rust-lang.org/rustc/platform-support.html>
- **TLS roots and time zones.** If the service makes outbound TLS calls, check that the runtime image has the CA
  certificates your TLS library reads, and tzdata if you need named time zones.
- **No shell.** You cannot exec into the container for debugging. The `:debug` image tags add a busybox shell; use
  them in a debugging copy, not in production.
- **Architecture.** An image built on an arm64 laptop is arm64. Cloud targets are often amd64: build with
  `--platform linux/amd64` or build in CI.

## 4. Health, readiness and startup probes

<https://kubernetes.io/docs/concepts/configuration/liveness-readiness-startup-probes/>

| Probe | Question | On failure | What it should check |
|---|---|---|---|
| startup | has it finished booting? | keeps waiting, then restarts | the same cheap handler as liveness |
| liveness | is the process wedged? | restart | the process itself ONLY |
| readiness | should it get traffic now? | removed from load balancing | warm-up done, not shutting down |

- **Never put the database in liveness.** A database blip would restart every replica at once and turn a small
  outage into a large one.
- Be careful even in readiness: if every replica fails readiness for a shared dependency, you have no backends.
  Often it is better to stay ready and return a clear error for the affected routes.
- Serve probes from the same runtime as real traffic, so a blocked runtime fails them.

```yaml
startupProbe:   { httpGet: { path: /livez,  port: 8080 }, periodSeconds: 2, failureThreshold: 30 }
livenessProbe:  { httpGet: { path: /livez,  port: 8080 }, periodSeconds: 10, failureThreshold: 3 }
readinessProbe: { httpGet: { path: /readyz, port: 8080 }, periodSeconds: 5,  failureThreshold: 2 }
```

## 5. Graceful shutdown

Order: receive SIGTERM, fail readiness, keep serving for a short drain delay while load balancers notice, stop
accepting, finish in-flight requests with a deadline, stop background workers, flush telemetry, exit 0.

```rust
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

async fn shutdown_signal(shutting_down: Arc<AtomicBool>, drain_delay: Duration) {
    use tokio::signal::unix::{SignalKind, signal};
    let mut term = signal(SignalKind::terminate()).expect("install SIGTERM handler");
    let mut int = signal(SignalKind::interrupt()).expect("install SIGINT handler");
    tokio::select! {
        _ = term.recv() => {}
        _ = int.recv() => {}
    }
    shutting_down.store(true, Ordering::SeqCst); // /readyz now answers 503
    tokio::time::sleep(drain_delay).await;
}

pub async fn serve(
    listener: tokio::net::TcpListener,
    app: axum::Router,
    shutting_down: Arc<AtomicBool>,
) -> std::io::Result<()> {
    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal(shutting_down, Duration::from_secs(5)))
        .await
}
```

- Graceful shutdown waits for open connections, and streaming or long-polling requests may never finish. Put a hard
  overall timeout around it that starts when the signal arrives, and tell long-lived streams to close.
- Background tasks need their own cancellation signal and must be awaited. Work that must not be lost needs a
  durable queue, because SIGKILL and machine failure give no warning.
- Docs: <https://docs.rs/tokio/latest/tokio/signal/unix/index.html>,
  <https://docs.rs/axum/latest/axum/serve/struct.Serve.html>, <https://tokio.rs/tokio/topics/shutdown>

**Kubernetes** (<https://kubernetes.io/docs/concepts/workloads/pods/pod-lifecycle/>): the default grace period is
30 seconds. A `preStop` hook runs before SIGTERM and its time counts inside the same grace period. Endpoint removal
happens concurrently with termination, which is why the drain delay exists. Keep
`drain delay + request deadline + flush time < terminationGracePeriodSeconds`.

**Cloud Run** (<https://cloud.google.com/run/docs/container-contract>): it sends SIGTERM and then SIGKILL after
10 seconds. Budget the whole sequence inside that, so use a much shorter drain delay than above.

## 6. Configuration and secrets

- Configuration comes from the environment (<https://12factor.net/config>). Parse all of it once at startup into a
  typed struct, and exit with a clear message if something is missing or invalid.
- Listen on `0.0.0.0:$PORT` (Cloud Run injects `PORT`).
- Never put secrets in the image, in build arguments, in Git or in logs. Give secret fields a type whose `Debug`
  prints `[redacted]`.
- Kubernetes Secrets are base64, which is not encryption. Enable encryption at rest, restrict RBAC, and prefer
  mounted files to environment variables (<https://kubernetes.io/docs/concepts/configuration/secret/>).
- Cloud Run with Secret Manager (<https://cloud.google.com/run/docs/configuring/services/secrets>): a secret in an
  **environment variable** is resolved when the instance starts, so pin a version number rather than `latest`. A
  secret mounted as a **volume** is fetched when read, which suits rotation, provided your code re-reads the file.

## 7. Logging and tracing

- Write one JSON object per line to stdout and let the platform collect it. `tracing` plus `tracing-subscriber` with
  its `json` feature does this (<https://docs.rs/tracing-subscriber/latest/tracing_subscriber/>); filter with
  `EnvFilter` (the `env-filter` feature) so levels change without a rebuild.
- On Google Cloud, use the field names it recognises (`severity`, `message`, trace fields), or levels and trace
  grouping will be wrong (<https://cloud.google.com/logging/docs/structured-logging>).
- Put a request id and trace id on a span around each request, and propagate the incoming trace context to outbound
  calls (OpenTelemetry). Use `.instrument(span)` for futures; do not hold `span.enter()` across an `.await`.
- Do not log secrets, tokens or request bodies with personal data. Escape user-controlled text (see
  `examples/web/security_pitfalls_secure_coding.rs`).
- Logging must not block the runtime. If you buffer through a background writer, flush it during shutdown.

## 8. Resource limits

<https://kubernetes.io/docs/concepts/configuration/manage-resources-containers/>

- **CPU limits throttle.** A container that exhausts its quota is paused until the next period, which appears as tail
  latency with a low average CPU graph. Watch the throttling counters. Many teams set CPU requests and omit CPU
  limits for latency-sensitive services; that is a cluster policy decision.
- **Thread pools.** `std::thread::available_parallelism` takes Linux affinity and cgroup quotas into account in the
  common cases, and its documentation lists cases where it over- or under-counts
  (<https://doc.rust-lang.org/std/thread/fn.available_parallelism.html>). Log the detected value at startup. Set
  tokio `worker_threads` and rayon threads explicitly if it is wrong for you, and remember that every library pool
  adds up.
- **Blocking work** belongs in `spawn_blocking` or a dedicated pool, with your own concurrency bound: the default
  blocking pool limit is large.
- **Memory limits kill.** Exceeding one gets the process OOM-killed with no unwinding and no shutdown handler. Bound
  every queue, cache, body size and connection pool. Alert on working-set memory well below the limit.
- **Allocators** may keep freed memory instead of returning it to the OS. Distinguish that from a leak with a heap
  profiler before tuning.
- Check file descriptor limits against your connection count.
- Size database pools globally: `replicas x pool size` (including surge replicas during a rollout) must stay below
  the server's connection limit.

## 9. Database migrations: expand and contract

During a rolling deploy, old and new code run at the same time, so every schema version must work with both the
current and the previous release. See <https://martinfowler.com/bliki/ParallelChange.html> and
`examples/database/online_schema_change.rs`.

1. **Expand:** add the new column or table, nullable or with a safe default. Old code ignores it.
2. Deploy code that writes both old and new.
3. **Backfill** in small batches, resumable and throttled, without overwriting newer values (the race is shown in
   `examples/microservices/strangler_fig_shadow_traffic.rs`).
4. Verify old against new.
5. Deploy code that reads the new shape. Rollback is still possible because both are written.
6. Stop writing the old shape.
7. **Contract:** drop the old column in a later release, once no running or rollback-target version uses it.

Run migrations as a separate pipeline step or job, not from every replica at startup. On Postgres, set
`lock_timeout` so a blocked DDL statement fails instead of queueing all later queries behind it, and build indexes
on live tables with `CREATE INDEX CONCURRENTLY` (it cannot run inside a transaction, and a failed run leaves an
invalid index to drop). Do not rely on down-migrations to undo data loss.

## 10. Observability checklist

- [ ] Per route: request rate, error rate, latency **histogram** (not only averages).
- [ ] Saturation: in-flight requests, queue depth, pool wait time, CPU throttling, memory working set.
- [ ] Per dependency: rate, errors, latency, retries, circuit breaker state.
- [ ] Runtime and process: panics, restarts, OOM kills.
- [ ] Bounded label values: no user ids, raw paths or error strings as labels.
- [ ] Build version on a metric and in the first log line, so graphs can be matched to deploys.
- [ ] Traces sampled, with errors and slow requests kept.
- [ ] A dashboard that answers "is it us or a dependency?" without writing new queries.

Background: `examples/dev_tools/metrics_prometheus_histograms_load_testing.rs` (including why a closed-loop load
test hides stalls).

## 11. SLOs and alerts

<https://sre.google/sre-book/service-level-objectives/> and <https://sre.google/workbook/alerting-on-slos/>

- Define an SLI as `good events / valid events` measured close to the user, then pick a target and window, for
  example 99.9% successful over 30 days. The error budget is the remaining 0.1% (about 43 minutes of full outage).
- Page on **burn rate** (how fast the budget is being spent) using a long and a short window together. The workbook's
  starting point for 99.9% is to page at 14.4x over 1 hour (confirmed over 5 minutes) and at 6x over 6 hours
  (confirmed over 30 minutes), and to open a ticket at 1x over 3 days.
- Page on user-visible symptoms. Causes such as high CPU belong on dashboards.
- Every page has an owner and a runbook: what it means, first checks, how to roll back, when to escalate.
- Alert on missing data as well.

**Rollout and rollback:** deploy gradually (canary), compare against the current version, and roll back first and
debug second. Confirm that rollback still works with the current schema.

## 12. Backup, restore and disaster recovery

<https://sre.google/sre-book/data-integrity/>

- Decide **RPO** (how much data you may lose) and **RTO** (how long recovery may take) before choosing tools.
- Replication is not backup: it copies a bad `DELETE` immediately.
- Postgres: automated backups plus point-in-time recovery
  (<https://cloud.google.com/sql/docs/postgres/backup-recovery/pitr>). Keep a copy that the credentials of the
  production service cannot delete.
- **Restore drill**, on a schedule: restore into a new instance, run integrity queries, point a staging service at
  it, record how long it took and how much data was missing, compare with RTO and RPO, and fix the runbook.
- DR scenarios to rehearse: bad migration, accidental deletion, lost instance, lost zone, lost region, leaked
  credential. Keep the runbook somewhere that survives the outage it describes.

## 13. Google Cloud specifics

**Cloud Run or GKE?** Start with Cloud Run for stateless HTTP or gRPC services: no cluster to operate. Choose GKE
when you need long-running workers with fine control, DaemonSets or StatefulSets, custom networking, or you already
run Kubernetes.

### Cloud Run

- Listen on `$PORT`, handle SIGTERM within 10 seconds, keep no required state on local disk.
- CPU allocation outside requests depends on the billing setting, so do not assume background tasks keep running
  after a response. Check the current documentation for your setting.
- Set concurrency per instance, minimum instances (cold starts) and maximum instances (this also caps database
  connections: `max instances x pool size`).
- Probes: <https://cloud.google.com/run/docs/configuring/healthchecks>
- Roll out with revisions: deploy with no traffic, send a small percentage, watch, increase, or send traffic back.

### GKE

- Use Workload Identity Federation so pods get Google credentials without key files
  (<https://cloud.google.com/kubernetes-engine/docs/concepts/workload-identity>).
- Set resource requests, a PodDisruptionBudget, a rolling update strategy, and spread replicas across zones.

### Cloud SQL

- From Cloud Run, grant the runtime service account the Cloud SQL Client role, attach the instance to the service,
  and connect over the Unix socket at `/cloudsql/PROJECT:REGION:INSTANCE`
  (<https://cloud.google.com/sql/docs/postgres/connect-run>). Postgres client libraries differ in whether they want
  the directory or the full `.s.PGSQL.5432` socket path.
- From GKE, use private IP or the Cloud SQL Auth Proxy.
- Use a high-availability configuration for production, enable point-in-time recovery, turn on deletion protection.

### Secret Manager

One service account per service. Grant `roles/secretmanager.secretAccessor` on the individual secrets it needs, not
on the project (<https://cloud.google.com/secret-manager/docs/overview>).

### Terraform skeleton

Layout: `infra/modules/service/` plus `infra/envs/{staging,prod}/`, one state per environment.

State lives in a Cloud Storage bucket with object versioning
(<https://developer.hashicorp.com/terraform/language/backend/gcs>). State can contain secret values, so restrict
access to it, and avoid putting secret **values** in Terraform at all: create the secret resource here and add
versions from a separate, access-controlled step.

| Resource | Purpose |
|---|---|
| `google_project_service` | enable the required APIs |
| `google_artifact_registry_repository` | container images |
| `google_service_account` | the runtime identity (not the default account) |
| `google_sql_database_instance`, `google_sql_database` | Postgres, with backups, PITR and deletion protection |
| `google_secret_manager_secret` | secret containers |
| `google_secret_manager_secret_iam_member` | per-secret access |
| `google_project_iam_member` | the Cloud SQL Client role |
| `google_cloud_run_v2_service` | the service: image digest, limits, scaling, probes, secrets, SQL volume |
| `google_monitoring_alert_policy` | alerts as code |

Resource reference:
<https://registry.terraform.io/providers/hashicorp/google/latest/docs/resources/cloud_run_v2_service>

Pipeline: format and validate, test, build once, push, record the digest, `terraform plan` (reviewed), apply to
staging, smoke test, then promote **the same digest** to production. Authenticate CI with Workload Identity
Federation, not a downloaded service account key.

## Pre-launch checklist

- [ ] `--locked` build, pinned toolchain, image deployed by digest
- [ ] overflow checks decided on purpose, panic strategy decided on purpose
- [ ] non-root image, no secrets in it
- [ ] liveness independent of dependencies; readiness fails during shutdown
- [ ] SIGTERM tested: send it under load and count failed requests
- [ ] every queue, pool, body size and timeout bounded
- [ ] connection budget computed for maximum scale plus rollout surge
- [ ] migration is backward compatible with the previous release
- [ ] SLO, burn-rate alerts, runbook, owner
- [ ] a restore drill has been performed and timed
- [ ] rollback rehearsed
