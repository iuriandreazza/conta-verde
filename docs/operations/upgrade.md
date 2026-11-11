---
title: Upgrading and going back
status: living
date: 2026-11-11
tags: [operations, upgrade]
---

# Upgrading and going back

What a release promises, and what it does not, is in [ADR 0092](../adr/0092-one-point-zero-hardening-plan.md). What matters here:

- **An upgrade is in place.** You start a newer release of the image on the same database and the same files; it applies the pending
  migrations at start, in order, each in its own transaction.
- **A migration is never undone.** There are no down migrations. **The way back is to restore the backup you took before upgrading.**
- A release refuses to start on a database a **later** release migrated (it says a migration is missing), and on one whose migration changed.
  So an old binary cannot be pointed at an upgraded database by mistake.
- Before the first tagged release nothing was promised: a database made by a development build is not supported.

## Upgrading

1. **Read the release notes** of every release between yours and the target. They say what changed in the settings (a variable that was
   deprecated, a new one) and anything an operator has to do.
2. **Check where you are.** `curl https://your-name/api/health` says the running version; the database's is
   `SELECT max(version) FROM _sqlx_migrations;`.
3. **Take a backup** ([backup-and-restore.md](backup-and-restore.md)) and **copy it off the host**. This is the way back: do not skip it.
4. **Change the release** in `production.env` (`CONTA_VERDE_VERSION=1.0.1`) and run:
   ```bash
   docker compose --env-file production.env pull app
   docker compose --env-file production.env up -d app
   docker compose --env-file production.env logs -f app
   ```
   The first lines say the migrations ran; `listening` says it is up. The proxy and the database are not restarted.
5. **Check:** `/api/health` says the new version, sign in, open the dashboard and one record you know. If you rehearsed it
   (below), the books of the rehearsal are what you look for.

Upgrade **one release at a time** unless the notes say a jump is safe: the rehearsal below proves each step in order.

### The settings may have changed

A variable that is **deprecated** in a minor makes the server log a warning at start naming its replacement, and is removed only in the next major.
A new variable is off or inert until you set it. Compare your `production.env` with `production.env.example` of the new release.

### Coming from a version that ran as one database role

Releases before the one that introduced row-level security ([ADR 0093](../adr/0093-row-level-security.md)) used one role for everything. The server of the
later releases **refuses to start as the owner of the tables**, with a message that says what to do: make the role that serves requests
(`deploy/postgres/create-app-role.sh`), give the server both addresses (`DATABASE_URL` for that role, `DATABASE_MIGRATE_URL` for the owner), and start it.
It grants the role its rights itself. The upgrade rehearsal below runs exactly this move.

## Going back

You cannot start an older release on the upgraded database. Instead restore the backup from step 3 into a **new project** and start the release you came from there, as
[backup-and-restore.md](backup-and-restore.md) describes (`docker compose -p conta-verde-restored ...`): stop the upgraded installation (`down`, not `down -v`), restore, and
start the restored project with `CONTA_VERDE_VERSION` set to the release you came from.

**Everything entered after the backup is lost.** That is the price of having no down migrations; it is why the backup comes first and why you look at the result before people
start working again after an upgrade. Keep the upgraded project's volumes until you are sure: they are what you would look at to recover what was entered meanwhile.

## Rehearsing an upgrade

`scripts/ops/rehearse-upgrade.sh` does the whole thing against a local PostgreSQL with any git refs: it builds each, makes an installation with the oldest, backs it up,
starts each following release on the same database **in place**, checks the books through the server and the row counts after every step, shows that the oldest
binary refuses the upgraded database, and restores the backup and starts the oldest binary on it.

```bash
scripts/ops/docker-pg-tools.sh /tmp/pg-tools && export PATH=/tmp/pg-tools:$PATH
OWNER_BASE_URL=postgres://conta_verde:...@127.0.0.1:5432 APP_BASE_URL=postgres://conta_verde_app:...@127.0.0.1:5432 \
scripts/ops/rehearse-upgrade.sh v1.0.0 v1.0.1 v1.1.0        # two consecutive upgrades
```

It builds every release (minutes) and is **run by hand before every tag** with the last released tag and the new one; its output goes with the release
notes. It is not in CI because it builds two or three releases. What it does **not** prove: a real host, a proxy, a managed database, data of a size
a real company has (the performance baseline is a separate document), or a release whose migration takes long on a big table.
