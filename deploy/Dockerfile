FROM node:22-bookworm-slim AS web
WORKDIR /src/web
COPY web/package.json web/package-lock.json ./
RUN npm ci
COPY web/ ./
COPY assets/item-icon-metadata/ /src/assets/item-icon-metadata/
COPY desktop/tauri.conf.json /src/desktop/tauri.conf.json
ARG POSTHOG_API_KEY
ARG NAUCTIONS_REVISION=unknown
RUN npm run build

FROM rust:1.93-bookworm AS rust
WORKDIR /src
COPY Cargo.toml Cargo.lock ./
COPY crates/ ./crates/
COPY db/ ./db/
COPY assets/ ./assets/
COPY desktop/tauri.conf.json ./desktop/tauri.conf.json
ARG NAUCTIONS_REVISION=unknown
RUN cargo build --locked --release -p market-server

FROM scratch AS web-symbols
COPY --from=web /src/artifacts/symbols/ /

FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates curl && rm -rf /var/lib/apt/lists/* \
    && useradd --system --uid 10001 --create-home market
WORKDIR /app
COPY --from=rust /src/target/release/market-server /usr/local/bin/market-server
COPY --from=web /src/web/dist ./web/dist/
COPY --from=web /src/artifacts/symbols/ /app/private-symbols/
USER market
ENV BIND_ADDR=0.0.0.0:8080 RUST_LOG=info
EXPOSE 8080
HEALTHCHECK --interval=30s --timeout=5s --start-period=30s CMD curl --fail --silent http://127.0.0.1:8080/healthz || exit 1
CMD ["market-server"]
