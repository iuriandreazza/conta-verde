# One image: nginx serves the web app and forwards /api to the API on loopback, so the browser sees a single origin
# (ADR 0005) and the API keeps its default bind address. It runs as an unprivileged user and listens on 3080; TLS is the
# job of a proxy in front of it (deploy/production, ADR 0096).
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
# The API embeds the document of /v1 (ADR 0087) at compile time.
COPY docs/api docs/api
RUN cargo build --release -p conta-verde-api

FROM debian:bookworm-slim
ARG VERSION=dev
LABEL org.opencontainers.image.title="conta-verde" \
      org.opencontainers.image.description="Open-source ERP for small businesses" \
      org.opencontainers.image.licenses="AGPL-3.0-or-later" \
      org.opencontainers.image.source="https://github.com/iuriandreazza/conta-verde" \
      org.opencontainers.image.version="$VERSION"
RUN apt-get update \
    && apt-get install -y --no-install-recommends nginx ca-certificates \
    && rm -rf /var/lib/apt/lists/* \
    && useradd --system --uid 10001 --home-dir /nonexistent --shell /usr/sbin/nologin conta-verde \
    && mkdir -p /data/attachments /tmp/nginx \
    && chown -R conta-verde /data /tmp/nginx
COPY --from=api /src/target/release/conta-verde /usr/local/bin/conta-verde
COPY --from=web /src/web/dist /usr/share/nginx/html
COPY deploy/nginx-main.conf /etc/nginx/nginx.conf
COPY deploy/nginx.conf /etc/nginx/conf.d/default.conf
COPY deploy/entrypoint.sh /entrypoint.sh
ENV BIND_ADDR=127.0.0.1:3000 ATTACHMENTS_DIR=/data/attachments
USER conta-verde
VOLUME /data
EXPOSE 3080
# bash is in the image and curl is not: the health answer is read over a plain socket.
HEALTHCHECK --interval=30s --timeout=3s --start-period=30s CMD ["/bin/bash", "-c", "exec 3<>/dev/tcp/127.0.0.1/3080 && printf 'GET /api/health HTTP/1.0\\r\\n\\r\\n' >&3 && grep -q '\"status\":\"ok\"' <&3"]
ENTRYPOINT ["/entrypoint.sh"]
