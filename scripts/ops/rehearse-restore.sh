#!/bin/bash
# Proves that a backup restores (ADR 0096): seeds an installation, backs it up, restores the backup into a fresh database and
# directory, compares the two by fingerprint, starts the server on the restored copy and checks the books through it.
#
#   OWNER_BASE_URL=postgres://conta_verde:conta_verde@127.0.0.1:5432 \
#   APP_BASE_URL=postgres://conta_verde_app:conta_verde_app@127.0.0.1:5432 \
#   API_BIN=target/debug/conta-verde scripts/ops/rehearse-restore.sh
#
# The role conta_verde_app must exist (deploy/postgres/create-app-role.sql). It makes and drops two databases whose names start with
# cv_rehearsal_ and uses a temporary directory; it touches nothing else.
set -euo pipefail

: "${OWNER_BASE_URL:?set OWNER_BASE_URL, the server and the owner role without a database name}"
: "${APP_BASE_URL:?set APP_BASE_URL, the server and the runtime role without a database name}"
API_BIN="${API_BIN:-target/debug/conta-verde}"
API_PORT="${API_PORT:-3190}"
here="$(cd "$(dirname "$0")" && pwd)"
work="$(mktemp -d)"
suffix="$(date +%s)"
db_a="cv_rehearsal_a_$suffix"
db_b="cv_rehearsal_b_$suffix"
server_pid=""

stop_server() {
    if [ -n "$server_pid" ]; then
        kill "$server_pid" 2>/dev/null || true
        wait "$server_pid" 2>/dev/null || true
        server_pid=""
    fi
}
cleanup() {
    stop_server
    psql -X -q "$OWNER_BASE_URL/postgres" -c "DROP DATABASE IF EXISTS $db_a WITH (FORCE)" -c "DROP DATABASE IF EXISTS $db_b WITH (FORCE)" -c "DROP DATABASE IF EXISTS ${db_b}_c WITH (FORCE)" >/dev/null 2>&1 || true
    rm -rf "$work"
}
trap cleanup EXIT

start_server() { # database, files directory
    DATABASE_URL="$APP_BASE_URL/$1" DATABASE_MIGRATE_URL="$OWNER_BASE_URL/$1" ATTACHMENTS_DIR="$2" \
        BIND_ADDR="127.0.0.1:$API_PORT" CNPJ_LOOKUP=off PUBLIC_URL=http://localhost:5173 \
        "$API_BIN" > "$work/server-$1.log" 2>&1 &
    server_pid=$!
    for _ in $(seq 1 60); do
        curl -sf "http://127.0.0.1:$API_PORT/health" >/dev/null && return 0
        kill -0 "$server_pid" 2>/dev/null || { echo "the server stopped:" >&2; tail -5 "$work/server-$1.log" >&2; exit 1; }
        sleep 1
    done
    echo "the server did not answer" >&2
    exit 1
}

echo "== an installation with books and a file"
psql -X -q "$OWNER_BASE_URL/postgres" -c "CREATE DATABASE $db_a"
mkdir -p "$work/files-a"
start_server "$db_a" "$work/files-a"
python3 "$here/check_books.py" seed "http://127.0.0.1:$API_PORT" "$work/state.json"
stop_server
"$here/fingerprint.sh" "$OWNER_BASE_URL/$db_a" > "$work/fingerprint-a.txt"

echo "== the backup"
BACKUP_DATABASE_URL="$OWNER_BASE_URL/$db_a" ATTACHMENTS_DIR="$work/files-a" BACKUP_DIR="$work/backups" "$here/backup.sh"
backup="$(ls -d "$work"/backups/conta-verde-* | tail -1)"

echo "== a restore into an empty database and directory"
psql -X -q "$OWNER_BASE_URL/postgres" -c "CREATE DATABASE $db_b"
RESTORE_DATABASE_URL="$OWNER_BASE_URL/$db_b" ATTACHMENTS_DIR="$work/files-b" "$here/restore.sh" "$backup"
"$here/fingerprint.sh" "$OWNER_BASE_URL/$db_b" > "$work/fingerprint-b.txt"
if ! diff "$work/fingerprint-a.txt" "$work/fingerprint-b.txt"; then
    echo "FAILED: the restored database is not the one that was backed up" >&2
    exit 1
fi
echo "  ok  the restored database has the same rows, policies and schema version ($(wc -l < "$work/fingerprint-a.txt") lines compared)"
if ! diff -r "$work/files-a" "$work/files-b"; then
    echo "FAILED: the restored files are not the ones that were backed up" >&2
    exit 1
fi
echo "  ok  the restored files are the same"

echo "== a restore is refused where something already is"
if RESTORE_DATABASE_URL="$OWNER_BASE_URL/$db_b" ATTACHMENTS_DIR="$work/files-c" "$here/restore.sh" "$backup" 2>/dev/null; then
    echo "FAILED: a restore into a database with tables was accepted" >&2
    exit 1
fi
echo "  ok  a restore into a database with tables is refused"
cp "$backup/db.dump" "$work/db.dump.keep"
printf 'x' >> "$backup/db.dump"
psql -X -q "$OWNER_BASE_URL/postgres" -c "CREATE DATABASE ${db_b}_c" >/dev/null
if RESTORE_DATABASE_URL="$OWNER_BASE_URL/${db_b}_c" ATTACHMENTS_DIR="$work/files-d" "$here/restore.sh" "$backup" 2>/dev/null; then
    echo "FAILED: a damaged backup was accepted" >&2
    exit 1
fi
echo "  ok  a backup that was changed is refused by its checksum"
psql -X -q "$OWNER_BASE_URL/postgres" -c "DROP DATABASE ${db_b}_c" >/dev/null
cp "$work/db.dump.keep" "$backup/db.dump"

echo "== the server on the restored copy"
start_server "$db_b" "$work/files-b"
python3 "$here/check_books.py" verify "http://127.0.0.1:$API_PORT" "$work/state.json"
stop_server
echo "restore rehearsal ok"
