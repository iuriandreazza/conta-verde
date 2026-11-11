---
title: Backup and restore
status: living
date: 2026-11-11
tags: [operations, backup]
---

# Backup and restore

A conta-verde installation is two things that must be backed up **together**: the PostgreSQL database and the directory of files
(`ATTACHMENTS_DIR`, `/data/attachments` in the image, the `attachments` volume in the compose file), which holds the only copy of the
files attached to entries and of the fiscal XML and PDF. The records of the files are in the database and their bytes are on disk, so a
copy of one without the other is not a backup.

A third thing is **not** in a backup on purpose: the secrets in the environment, above all `FISCAL_SECRETS_KEY`. Keep them in your
password manager, apart from the backups.

## Taking a backup

```bash
BACKUP_DATABASE_URL=postgres://conta_verde:OWNER_PASSWORD@localhost:5432/conta_verde \
ATTACHMENTS_DIR=/path/to/the/attachments/volume \
BACKUP_DIR=/backups \
PAUSE_CMD="docker compose stop app" RESUME_CMD="docker compose start app" \
scripts/ops/backup.sh
```

It writes `/backups/conta-verde-<UTC time>/` with `db.dump` (a `pg_dump` in custom format), `attachments.tar` and `manifest.txt` (the
schema version, and a checksum of each file). Notes:

- **It must run as the schema's owner** (`conta_verde`), not as the role the application uses. The row-level policies hide every row from the
  application's role, so a dump taken as it would be empty ([ADR 0093](../adr/0093-row-level-security.md)).
- **`PAUSE_CMD` and `RESUME_CMD`** stop the application for the seconds the copy takes, which is what makes every file exactly the one its record
  names. Without them the dump is taken first and the files after: the worst that can happen is a record whose file was removed in between,
  which the entry then shows as unavailable. Everything else about taking it online is safe.
- You need `pg_dump` of PostgreSQL **17 or newer** (the server's version). If the host has none, `scripts/ops/docker-pg-tools.sh DIR` makes
  wrappers that run the tools of the `postgres:17` image on the host's network. Or set `PG_DUMP` and `PSQL` to your own commands, for example
  `PG_DUMP="docker compose exec -T db pg_dump -U conta_verde conta_verde"`.
- Schedule it (cron, a systemd timer) and **copy the result off the host** (another machine, object storage): a backup on the same disk is
  not one. Keep several, and delete old ones by your own rule; the tool deletes nothing.

## Restoring

A restore goes into an **empty** database and an empty directory; it refuses anything else and checks the checksums of the backup first.

```bash
# 1. an empty database, reachable as the role that will own the schema
docker compose exec db createdb -U conta_verde conta_verde_restored       # or create it as you do

# 2. restore
RESTORE_DATABASE_URL=postgres://conta_verde:OWNER_PASSWORD@localhost:5432/conta_verde_restored \
ATTACHMENTS_DIR=/path/to/an/empty/directory \
scripts/ops/restore.sh /backups/conta-verde-20270101T030000Z

# 3. point the application at them (DATABASE_URL, DATABASE_MIGRATE_URL, ATTACHMENTS_DIR) and start it
```

- The role the application serves requests as (`conta_verde_app`) must exist in the cluster: it is not in a dump. On a new cluster, run
  `deploy/postgres/create-app-role.sh` first. The application gives it its rights again at start, so a restore needs no `GRANT`s.
- **Start the release the backup was made with, or a later one.** A later release migrates the restored database at start. An earlier one
  refuses it ([upgrade.md](upgrade.md)).
- Put `FISCAL_SECRETS_KEY` back in the environment, the same one.
- To move to another host this is all there is: back up, restore there, start.

## Proving it works

Do not wait for the day you need it. `scripts/ops/rehearse-restore.sh` seeds an installation (a user, a company, entries one of them settled,
a stock movement and a file), backs it up, restores it into a fresh database and directory, compares **every row** of the two by hash
(`scripts/ops/fingerprint.sh`), compares the files, checks that a restore into a database with tables and a backup that was changed are both
refused, and starts the server on the restored copy and checks the books through it, signing in with the same password.

```bash
scripts/ops/docker-pg-tools.sh /tmp/pg-tools && export PATH=/tmp/pg-tools:$PATH   # if the host has no PostgreSQL 17 tools
OWNER_BASE_URL=postgres://conta_verde:...@127.0.0.1:5432 APP_BASE_URL=postgres://conta_verde_app:...@127.0.0.1:5432 \
API_BIN=target/debug/conta-verde scripts/ops/rehearse-restore.sh
```

It runs in CI for every change (`.github/workflows/ci.yml`). Run the same on your own host's backup tooling the first week, and **restore a real backup once** into a
scratch database: that is the only proof a backup you took is one you can use.

## What comes back, and what runs again

Everything in the dump: the books, the audit trail, every company's settings, the sessions that had not expired, and the **job queue as it was**.
The queue is the one to think about: an e-mail or a webhook delivery that was waiting when the backup was taken is sent after the restore, and one
that was sent since is not known to have been (jobs are delivered **at least once**, [ADR 0059](../adr/0059-job-queue.md)), so a restore to an earlier
moment can send a message again. Restoring to a copy you only want to look at? Start it without `SMTP_URL` and without webhook subscriptions
reachable, or empty the `jobs` table first. The files are exactly those in `attachments.tar`; one added to the directory after the backup is not.
