---
title: "ADR 0096: The image, the backup, the restore and the upgrade"
status: accepted
date: 2026-11-11
tags: [adr, operations, release, phase-9]
---

# ADR 0096: The image, the backup, the restore and the upgrade

## Context

[ADR 0092](0092-one-point-zero-hardening-plan.md) promises, for 1.0, forward-only migrations applied at start, restore as the only way back, and
stable configuration, and asks for a published image, a backup and restore procedure with a script that proves it, and an upgrade guide whose
exit criterion is "an operator upgrades two consecutive versions in place with a restored backup". [ADR 0093](0093-row-level-security.md) changed what a
deployment is (two database roles). What existed: a `Dockerfile` and an nginx file for the **development** environment (root, one database
role), a compose file for a development database, and nothing that said how to back anything up. Three facts shape the design:

- **The books are a database and a directory.** The records of the attached files and of the fiscal files are in PostgreSQL and their bytes are in
  `ATTACHMENTS_DIR`, written **before** their record and dropped **after** a record marks them removed ([ADR 0036](0036-entry-attachments.md)). A backup
  has to take both, in an order that leaves the worst case harmless.
- **The row-level policies make `pg_dump` the owner's job.** A dump taken as the role the application uses is empty by design.
- **No version has ever been tagged**, so the publishing workflow cannot be run before the first tag, and an upgrade between two releases can only be
  rehearsed between two commits until then.

## Options considered

| Question | Options | Chosen, and why |
|----------|---------|-----------------|
| One image or several | API image + web image + proxy; **one image with the web server and the API** | One: it is what the development environment already runs, the browser sees one origin ([ADR 0005](0005-spa-frontend.md)), and an operator pulls one thing. The cost is that the web server and the API share a container's lifecycle; the entrypoint keeps the web server up when the API dies so the logs say why |
| Root or not | Root; **an unprivileged user (uid 10001)** | Unprivileged: nginx runs with its pid and temporary files in `/tmp`, listening on 3080, the API on loopback. Small and `debian:bookworm-slim` based; a distroless image would drop `bash` (used by the health check) and nginx |
| The proxy in the example | Caddy; nginx; Traefik | **Caddy**: certificates with no configuration. The image's own nginx does the part that must hold whatever the proxy is: the **rate limit on sign-in, sign-up, password reset and invitations** (30 a minute per client, burst 10), since the API has none ([ADR 0092](0092-one-point-zero-hardening-plan.md) leaves it to the proxy), with the client's address read from `X-Forwarded-For` only for requests from a private network |
| The orchestrator | Compose; Kubernetes manifests; a systemd unit | **Compose**, one file an operator can read. Nothing in the product depends on it |
| Consistency of a backup | Always stop the application; take it online; filesystem or volume snapshots | A script that takes the dump first and the files after, with **hooks to stop and start the application** around the copy (`PAUSE_CMD`, `RESUME_CMD`). Online is safe except for a rare record whose file was removed in the gap; stopped is exact. A volume snapshot is the operator's own tool and is not scripted |
| The way back | Down migrations; a compatibility window; **restore** | Restore, as ADR 0092 decided. The upgrade guide puts the backup first |
| How the upgrade is proven | A CI job; **a script run by hand before each tag** | A script. It builds two or three releases from git refs, which takes minutes, so CI runs only the restore rehearsal (one build, reused) and a maintainer runs the upgrade rehearsal before tagging |

## Decision

### The image

`Dockerfile`: a web build stage, a release build of the API (it copies `docs/api`, which the API embeds, [ADR 0087](0087-public-api-v1.md)), and a final
stage with nginx and the binary running as `conta-verde` (uid 10001), `VOLUME /data`, `EXPOSE 3080`, OCI labels (the version is the `VERSION` build argument),
and a health check that reads `GET /api/health` over a plain socket (`curl` is not in the image). `GET /health` answers
`{"status":"ok","version":"<Cargo version>"}` so an operator can tell which release is running. `deploy/entrypoint.sh` is unchanged in kind: it starts the API in the background and
nginx in the foreground; **it still sets `ALLOW_PRIVILEGED_DATABASE_ROLE=true` when no `DATABASE_MIGRATE_URL` is given**, which is the development environment's single-role
database (the platform injects one role); a production deployment gives both URLs and gets the barrier. This is a known compromise of one image serving two uses,
written here and in `docs/operations/configuration.md`; the compose example always sets both.

### Publishing

`.github/workflows/release.yml` runs **only for a tag `v*`**, needs **only `GITHUB_TOKEN`** (`packages: write`), checks that the tag is the version in `Cargo.toml` (which is what `/api/health` says), and pushes
`ghcr.io/<owner>/conta-verde:<version>` and `<major>.<minor>` (no `latest`: nobody upgrades by pulling; a pre-release tag gets no `<major>.<minor>`), for `linux/amd64`.
`ci.yml` builds the same image on every change without pushing it and checks that the production compose file is valid.
**The workflow has never run**: it cannot before the first tag, and no test can push to a registry without a credential. It was written from the actions' documentation and is read, not run.

### The production example

`deploy/production/`: `docker-compose.yml` (db, app, Caddy proxy; only the proxy publishes ports; `COOKIE_SECURE=true`; the owner and runtime passwords from the environment; a volume for the
files), `Caddyfile`, `production.env.example`, and `deploy/postgres/create-app-role.sh`, which makes the runtime role with a password from the environment when the database is first created (run
from `/docker-entrypoint-initdb.d`) and may be run again. The development compose keeps `create-app-role.sql` with its development password.

### Backup, restore, fingerprint

`scripts/ops/backup.sh` writes `db.dump` (custom format, `--no-owner --no-acl`, **as the owner**), `attachments.tar` and a `manifest.txt` with the schema version and checksums; it deletes nothing and
does not carry the environment's secrets (**`FISCAL_SECRETS_KEY` is not in the dump and not in the backup**: without it sealed settings cannot be opened, which the docs say first).
`scripts/ops/restore.sh` verifies the checksums, **refuses a database with tables or a directory with files**, restores, and compares the restored schema version with the manifest. The role the application serves
requests as is a cluster object and not in a dump: it must exist, and the server grants it its rights again at start ([ADR 0093](0093-row-level-security.md)), so a restore needs no grants. `scripts/ops/fingerprint.sh` prints,
for every table, its row count and a hash of every row, plus the number of policies and the schema version, leaving out what a running server rewrites on its own (sessions, jobs, the webhook delivery log).

### The rehearsals

- `scripts/ops/rehearse-restore.sh` seeds an installation through the server (`scripts/ops/check_books.py seed`: a user, a company, a customer, a product, two entries one settled, a stock movement and a 4 KiB file), backs it up, restores it into a fresh
  database and directory, **compares every row by fingerprint and the files byte for byte**, checks that a restore into a database with tables and a changed backup are both refused, starts the server on the copy and checks the books through it,
  signing in with the same password. It **runs in CI**.
- `scripts/ops/rehearse-upgrade.sh OLDEST [MIDDLE...] NEWEST` builds the refs, makes the installation with the oldest, backs it up, starts each following release **in place** on the same database (checking the books through the server and the
  row counts), shows that the oldest binary **refuses** the upgraded database, and restores the pre-upgrade backup and starts the oldest binary on it. A release before row-level security runs as the owner and one after it as the runtime role, so the move
  every existing operator makes is rehearsed. It is **run by hand before every tag** and its output kept with the release notes.
- Result of this slice's run: the oldest release of Phase 8 (`623cbdc`, before ADRs 0092 to 0096) upgraded in place to this slice (`HEAD`), then the same installation upgraded through a middle release (`194a01c`) to `HEAD`: books, balances, stock and the attached file checked through the server after each step, row counts equal, the oldest binary refusing the upgraded database, and the pre-upgrade backup restored and started with the oldest binary. See `docs/operations/upgrade.md` for what it does not prove

### What this does not settle

- **The exit criterion of Phase 9 is met in two steps** ([ADR 0092](0092-one-point-zero-hardening-plan.md)): the rehearsal between two commits now; the first real upgrade between two tags closes it, and the roadmap item stays open until then.
- Not rehearsed: a real host, a proxy and a certificate, a managed database service (its roles and dump rights differ), several instances, data of a size a company has, a migration that takes long on a big table, the first publication of the image.
- A backup is a script and a documented procedure; scheduling, copying it off the host and **restoring a real one once** are the operator's.
- Multi-architecture images (`linux/arm64`) are not built.

## Consequences

- A release must be tagged with the Cargo version (`/api/health`, the image tag and the workflow agree). Bumping the version is part of cutting a release.
- The dump is the owner's: a managed database whose owner cannot be given to the application is configured as ADR 0093 says (an owner role and a runtime role), and its backup tool must run as the owner or a role that bypasses policies.
- A restore brings back the job queue as it was: at-least-once delivery means a message may be sent again after restoring to an earlier moment. The guide says so.
- Every migration of 1.x is applied by the server at start, under an advisory lock, as the owner; a release that needs a slow migration must say so in its notes.

## Revisit when

An operator needs point-in-time recovery (WAL archiving; the dump is a snapshot), several instances behind a balancer (shared rate limits, one hourly pass), an architecture other than amd64, or the upgrade
rehearsal becomes cheap enough to run in CI.
