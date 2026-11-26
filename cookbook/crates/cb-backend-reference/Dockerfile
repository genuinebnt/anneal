# NOT YET BUILT OR RUN: written to illustrate the shape of a production image.
# This crate is a member of the cookbook workspace, so the build context is the cookbook directory:
#   docker build -f crates/cb-backend-reference/Dockerfile -t backend-reference .

FROM rust:1.98-bookworm AS build
WORKDIR /src
COPY . .
RUN cargo build --release --locked -p cb-backend-reference --bin backend-demo

FROM debian:bookworm-slim
# No shell tools, secrets or build chain are added. SQLite is compiled into the binary.
RUN useradd --system --uid 10001 --no-create-home app \
    && install -d -o app -g app /data
COPY --from=build /src/target/release/backend-demo /usr/local/bin/backend-demo
ENV APP__SERVER__BIND=0.0.0.0:8080 \
    APP__DB__PATH=/data/app.db \
    APP__STORAGE__DIR=/data/objects
USER 10001
VOLUME ["/data"]
EXPOSE 8080
# The binary checks itself, so the image needs no curl.
HEALTHCHECK --interval=10s --timeout=5s --retries=3 CMD ["backend-demo", "healthcheck"]
# Exec form: the process is PID 1 and receives SIGTERM, which starts the graceful drain.
ENTRYPOINT ["backend-demo"]
CMD ["serve"]
