---
title: Running conta-verde
status: living
date: 2026-11-11
tags: [operations, guide]
---

# Running conta-verde

For the person who hosts an installation. The product is one image (the web app and the API behind a small web server, running as an
unprivileged user on port 3080) and a PostgreSQL database; nothing else is required. The decisions behind this page are in
[ADR 0092](../adr/0092-one-point-zero-hardening-plan.md) (what a release promises) and [ADR 0096](../adr/0096-operations.md) (the image, the backup
and the upgrade).

| You want to | Read |
|-------------|------|
| Run it on a server with TLS | [Installing](#installing) |
| Know what every setting does | [configuration.md](configuration.md) |
| Take a backup, restore one, move to another host | [backup-and-restore.md](backup-and-restore.md) |
| Move to a newer release, or back | [upgrade.md](upgrade.md) |

## Installing

You need a host with Docker (and the Compose plugin), a name that points at it, and ports 80 and 443 open. The files are in
[`deploy/production/`](../../deploy/production/).

```bash
cd deploy/production
cp production.env.example production.env      # fill it in: the release, the name, two passwords, FISCAL_SECRETS_KEY
docker compose --env-file production.env up -d
```

What starts:

- **`db`**: PostgreSQL 17. The first time, it makes the database, its owner (`conta_verde`) and the role the application serves requests
  as (`conta_verde_app`, which owns nothing: it is what lets the row-level security of [ADR 0093](../adr/0093-row-level-security.md) hold).
- **`app`**: the image `ghcr.io/iuriandreazza/conta-verde:<release>`. At start it migrates the database as the owner, gives the other role
  its rights, checks it is not running as a role the policies do not bind (it refuses to start otherwise), and listens.
- **`proxy`**: Caddy, which gets and renews the certificate and sends everything to `app:3080`. Any proxy will do that forwards the same way and sets
  `X-Forwarded-For` and `X-Forwarded-Proto`. The image's own web server limits sign-in, sign-up, password-reset and invitation routes to 30 a
  minute per client with a burst of 10, because the API has no rate limiting of its own.

The image is published to `ghcr.io/iuriandreazza/conta-verde`. The first time a package is pushed to ghcr it is **private**: the owner of the repository makes it public in the package's
settings (or the host logs in with `docker login ghcr.io`) before `docker compose pull` works.

Open `https://your-name`, create the first account (it creates the first company) and invite the people who will use it. Without `SMTP_URL`
and `SMTP_FROM` no e-mail is sent: invitation links are shown on the screen to pass on yourself.

What you must keep, and where:

| What | Where it lives | In the backup? |
|------|----------------|----------------|
| The books | the `db-data` volume (PostgreSQL) | yes (`db.dump`) |
| Files attached to entries, fiscal XML and PDF | the `attachments` volume (`/data/attachments`) | yes (`attachments.tar`) |
| **`FISCAL_SECRETS_KEY`** and the other secrets of `production.env` | your own records | **no: keep them apart** |

Losing `FISCAL_SECRETS_KEY` does not lose the books, but what it sealed (a company's gateway settings, webhook secrets) can no longer be opened and
has to be entered again.

## What is checked here and what is not

The image builds in CI on every change and the publishing workflow runs only for a version tag. The restore and the upgrade are
rehearsed by scripts ([backup-and-restore.md](backup-and-restore.md), [upgrade.md](upgrade.md)) on a developer's machine, against a local
PostgreSQL. **Not rehearsed**: a real host, a certificate, a managed database service (its roles and `pg_dump` rights differ), several
instances behind a balancer (the `v1` rate limit and the hourly passes are per process), and the first publication of the image.

## Version and health

`GET /api/health` answers `{"status":"ok","version":"1.0.0"}`: the version of the code that is running. The version of the database is
`SELECT max(version) FROM _sqlx_migrations;`. The container's health check reads the same answer.

Logs go to the container's output (`docker compose logs app`); `RUST_LOG=debug` makes them talk more.
