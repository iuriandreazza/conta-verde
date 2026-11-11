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

With the compose installation of [README.md](README.md) (run from `deploy/production`; this is the sequence the rehearsal in the repository runs):

```bash
COMPOSE="docker compose --env-file production.env"
BACKUP_DIR=/backups \
PG_DUMP="$COMPOSE exec -T db pg_dump -U conta_verde conta_verde" \
PSQL="$COMPOSE exec -T db psql -U conta_verde -d conta_verde -A -t -q" \
ATTACHMENTS_TAR_CMD="$COMPOSE run --rm -T --no-deps --entrypoint tar app -C /data/attachments -cf - ." \
PAUSE_CMD="$COMPOSE stop app" RESUME_CMD="$COMPOSE start app" \
../../scripts/ops/backup.sh
```

It writes `/backups/conta-verde-<UTC time>/` with `db.dump` (a `pg_dump` in custom format), `attachments.tar` and `manifest.txt` (the schema version, and a checksum of
each file). For a database and a directory that are not in containers, give `BACKUP_DATABASE_URL` (the owner's) and `ATTACHMENTS_DIR` instead of the commands. Notes:

- **It must run as the schema's owner** (`conta_verde`), not as the role the application uses. The row-level policies hide every row from the application's role,
  so a dump taken as it would be empty ([ADR 0093](../adr/0093-row-level-security.md)).
- **`PAUSE_CMD` and `RESUME_CMD`** stop the application for the seconds the copy takes, which is what makes every file exactly the one its record names. (The files are
  read by a throwaway container that mounts the same volume as the application's user, so this works with the application stopped.) Without them the dump is taken
  first and the files after: the worst that can happen is a record whose file was removed in between, which the entry then shows as unavailable.
- You need `pg_dump` of PostgreSQL **17 or newer**: with the commands above it is the database container's. For a host-installed client without a 17, `scripts/ops/docker-pg-tools.sh DIR`
  makes wrappers that run the image's tools on the host's network.
- Schedule it (cron, a systemd timer) and **copy the result off the host** (another machine, object storage): a backup on the same disk is not one. Keep several, and delete old ones
  by your own rule; the tool deletes nothing.

## Restoring

A restore goes into an **empty** database and an empty directory; it refuses anything else and checks the checksums of the backup first. With compose the simplest empty place is a
**new project**: a different project name makes new volumes, so nothing of the running installation is touched.

```bash
COMPOSE="docker compose --env-file production.env -p conta-verde-restored"
$COMPOSE up -d db                                   # a new, empty database; the first start makes the roles too
PG_RESTORE="$COMPOSE exec -T db pg_restore --no-owner --no-acl --exit-on-error -U conta_verde -d conta_verde" \
PSQL="$COMPOSE exec -T db psql -U conta_verde -d conta_verde -A -t -q" \
ATTACHMENTS_UNTAR_CMD="$COMPOSE run --rm -T --no-deps --entrypoint tar app -C /data/attachments -xf -" \
../../scripts/ops/restore.sh /backups/conta-verde-20270101T030000Z
$COMPOSE up -d app                                  # it migrates at start if the release is newer than the backup
```

- The files are unpacked by a container running as the application's user, so the application can read them. (Unpacking as another user would leave files it cannot open.)
- To look at the copy without taking over, leave the proxy out (as above). To **make it the installation** (a move to another host, or going back), stop the old one with
  `docker compose --env-file production.env down` (not `down -v`: that deletes the volumes), start the restored project **with its proxy** (`$COMPOSE up -d`), and from then on
  use `-p conta-verde-restored` for every command, or move the project name into `COMPOSE_PROJECT_NAME` in `production.env`.
- Without containers: `RESTORE_DATABASE_URL` (the owner's, an empty database) and `ATTACHMENTS_DIR` (an empty directory) instead. The role the application serves requests as
  (`conta_verde_app`) must exist in the cluster: it is not in a dump (`deploy/postgres/create-app-role.sh`). The application gives it its rights again at start, so a restore needs no `GRANT`s.
- **Start the release the backup was made with, or a later one.** A later release migrates the restored database at start. An earlier one refuses it ([upgrade.md](upgrade.md)).
- Put `FISCAL_SECRETS_KEY` back in the environment, the same one.

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
