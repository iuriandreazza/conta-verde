#!/bin/bash
# A backup of one conta-verde installation: the database and the files, taken together (ADR 0096, docs/operations/backup-and-restore.md).
#
#   BACKUP_DATABASE_URL=postgres://conta_verde:...@db:5432/conta_verde \
#   ATTACHMENTS_DIR=/var/lib/conta-verde/attachments BACKUP_DIR=/backups scripts/ops/backup.sh
#
# Writes BACKUP_DIR/conta-verde-<UTC time>/ holding db.dump (pg_dump, custom format, as the owner: the policies of ADR 0093 hide rows
# from any other role), attachments.tar (the files attached to entries and the fiscal files) and manifest.txt (what was backed up and
# its checksums). The environment's secrets, above all FISCAL_SECRETS_KEY, are NOT in it: keep them apart.
#
# For a copy in which every file is exactly the one its record names, the application must not write while the files are copied:
# PAUSE_CMD and RESUME_CMD (for example `docker compose stop app` and `docker compose start app`) are run around the copy. Without
# them the dump is taken first and the files after, which can leave a file with no record (harmless) and, rarely, a record whose
# file was removed in between (the entry then shows the attachment as unavailable).
#
# PG_DUMP and PSQL may be set to other commands, e.g. PG_DUMP="docker compose exec -T db pg_dump -U conta_verde conta_verde".
set -euo pipefail

: "${ATTACHMENTS_DIR:?set ATTACHMENTS_DIR}"
: "${BACKUP_DIR:?set BACKUP_DIR}"
if [ -z "${PG_DUMP:-}" ] || [ -z "${PSQL:-}" ]; then
    : "${BACKUP_DATABASE_URL:?set BACKUP_DATABASE_URL (the owner of the schema), or PG_DUMP and PSQL}"
fi
PG_DUMP="${PG_DUMP:-pg_dump $BACKUP_DATABASE_URL}"
PSQL="${PSQL:-psql -X -A -t -q $BACKUP_DATABASE_URL}"

stamp="$(date -u +%Y%m%dT%H%M%SZ)"
out="$BACKUP_DIR/conta-verde-$stamp"
mkdir -p "$out"

paused=0
resume() {
    if [ "$paused" = 1 ] && [ -n "${RESUME_CMD:-}" ]; then
        bash -c "$RESUME_CMD"
        paused=0
    fi
}
trap resume EXIT

if [ -n "${PAUSE_CMD:-}" ]; then
    bash -c "$PAUSE_CMD"
    paused=1
fi

# The dump first: a file written after it is an orphan nobody asks for, a file read before it could be missing from a record.
$PG_DUMP --format=custom --no-owner --no-acl > "$out/db.dump"
if [ -d "$ATTACHMENTS_DIR" ]; then
    tar -C "$ATTACHMENTS_DIR" -cf "$out/attachments.tar" .
else
    tar -cf "$out/attachments.tar" -T /dev/null
fi
resume

schema_version="$(echo "SELECT max(version) FROM _sqlx_migrations" | $PSQL)"
{
    echo "created_utc=$stamp"
    echo "schema_version=$schema_version"
    echo "paused_application=$([ -n "${PAUSE_CMD:-}" ] && echo yes || echo no)"
    (cd "$out" && sha256sum db.dump attachments.tar | sed 's/^/sha256 /')
} > "$out/manifest.txt"

echo "backup written to $out (schema version $schema_version)"
echo "not included, keep apart: FISCAL_SECRETS_KEY and the rest of the environment"
