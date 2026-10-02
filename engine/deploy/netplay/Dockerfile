# wf-signal (Matchbox signaling + room tokens + TURN credentials at /ice) and wf-turn (a small TURN relay), built
# from the engine workspace. Build from engine/:
#   docker build -f deploy/netplay/Dockerfile -t wf-netplay .
# Run:  docker run -p 3536:3536 wf-netplay                      (wf-signal, the default command)
#       docker run --network host wf-netplay wf-turn --secret S --public-ip 203.0.113.7
# No game content is in the image: the servers never see the ROM or game traffic.

FROM rust:1-bookworm AS build
WORKDIR /src
COPY Cargo.toml Cargo.lock ./
COPY crates crates
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/src/target \
    cargo build --release --locked -p wf-net --features server,turn --bin wf-signal --bin wf-turn \
 && mkdir -p /out && cp target/release/wf-signal target/release/wf-turn /out/

FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates curl && rm -rf /var/lib/apt/lists/* \
 && useradd --system --no-create-home --shell /usr/sbin/nologin wf
COPY --from=build /out/wf-signal /out/wf-turn /usr/local/bin/
USER wf
EXPOSE 3536/tcp 3478/udp
ENV WF_SIGNAL_ADDR=0.0.0.0:3536
HEALTHCHECK --interval=30s --timeout=3s CMD curl -fsS http://127.0.0.1:3536/healthz || exit 1
CMD ["wf-signal"]
