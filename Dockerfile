# One image for the dev environment: nginx serves the web app and forwards /api to the API on loopback,
# so the browser sees a single origin (ADR 0005) and the API keeps its default bind address.
FROM node:22-slim AS web
RUN corepack enable
WORKDIR /src
COPY package.json pnpm-lock.yaml pnpm-workspace.yaml ./
COPY web/package.json web/
RUN pnpm install --frozen-lockfile --filter conta-verde-web...
COPY web web
ARG VITE_SOURCE_URL=https://github.com/iuriandreazza/conta-verde
ENV VITE_SOURCE_URL=$VITE_SOURCE_URL
RUN pnpm --filter conta-verde-web build

FROM rust:1-slim-bookworm AS api
WORKDIR /src
COPY Cargo.toml Cargo.lock ./
COPY crates crates
RUN cargo build --release -p conta-verde-api

FROM debian:bookworm-slim
RUN apt-get update \
    && apt-get install -y --no-install-recommends nginx ca-certificates \
    && rm -rf /var/lib/apt/lists/*
COPY --from=api /src/target/release/conta-verde /usr/local/bin/conta-verde
COPY --from=web /src/web/dist /usr/share/nginx/html
COPY deploy/nginx.conf /etc/nginx/conf.d/default.conf
COPY deploy/entrypoint.sh /entrypoint.sh
ENV BIND_ADDR=127.0.0.1:3000 ATTACHMENTS_DIR=/data/attachments
EXPOSE 3080
ENTRYPOINT ["/entrypoint.sh"]
