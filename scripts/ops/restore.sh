#!/bin/bash
# Restores a backup made by backup.sh into an EMPTY database and an empty files directory (ADR 0096,
# docs/operations/backup-and-restore.md).
#
#   RESTORE_DATABASE_URL=postgres://conta_verde:...@db:5432/conta_verde_restored \
#   ATTACHMENTS_DIR=/var/lib/conta-verde/attachments scripts/ops/restore.sh /backups/conta-verde-20270101T030000Z
#
# The database must exist, be empty and be reachable as the role that is to own the schema, which is also the role the server
# migrates with (DATABASE_MIGRATE_URL). The role the server serves requests as (conta_verde_app, deploy/postgres/create-app-role.*) must
# exist in the cluster: it is a cluster object and a dump does not carry it. The server grants it its rights again at its next start.
# Nothing is overwritten: a database with tables or a files directory with files is refused.
set -euo pipefail

backup="${1:?usage: restore.sh BACKUP_DIR}"
: "${ATTACHMENTS_DIR:?set ATTACHMENTS_DIR}"
if [ -z "${PG_RESTORE:-}" ] || [ -z "${PSQL:-}" ]; then
    : "${RESTORE_DATABASE_URL:?set RESTORE_DATABASE_URL (the owner of the schema), or PG_RESTORE and PSQL}"
fi
PG_RESTORE="${PG_RESTORE:-pg_restore --no-owner --no-acl --exit-on-error --dbname $RESTORE_DATABASE_URL}"
PSQL="${PSQL:-psql -X -A -t -q $RESTORE_DATABASE_URL}"

[ -f "$backup/manifest.txt" ] || { echo "$backup has no manifest.txt: it is not a backup of this tool" >&2; exit 1; }
(cd "$backup" && sha256sum --check --quiet --ignore-missing <(grep '^sha256 ' manifest.txt | sed 's/^sha256 //')) \
    || { echo "a file of the backup does not match its checksum: it is damaged or was changed" >&2; exit 1; }

tables="$(echo "SELECT count(*) FROM pg_tables WHERE schemaname = 'public'" | $PSQL)"
if [ "$tables" != "0" ]; then
    echo "the database already has $tables tables: restore into an empty one" >&2
    exit 1
fi
if [ -d "$ATTACHMENTS_DIR" ] && [ -n "$(ls -A "$ATTACHMENTS_DIR")" ]; then
    echo "$ATTACHMENTS_DIR is not empty: restore into an empty directory" >&2
    exit 1
fi

$PG_RESTORE < "$backup/db.dump"
mkdir -p "$ATTACHMENTS_DIR"
tar -C "$ATTACHMENTS_DIR" -xf "$backup/attachments.tar"

expected="$(sed -n 's/^schema_version=//p' "$backup/manifest.txt")"
found="$(echo "SELECT max(version) FROM _sqlx_migrations" | $PSQL)"
if [ "$expected" != "$found" ]; then
    echo "the restored schema version is $found, the backup said $expected" >&2
    exit 1
fi
echo "restored (schema version $found). Start the application of the release the backup was made with, or a later one: it migrates at start."
