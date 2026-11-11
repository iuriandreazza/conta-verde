#!/bin/bash
# Rehearses an upgrade in place across releases (ADR 0096): the installation is made with the oldest of the given git refs, backed up,
# and then started with each following ref on the same database, which migrates it at start. After every step the books are checked
# through the server and the row counts compared; the oldest binary is then shown to refuse the upgraded database; and the backup taken
# before the upgrade is restored and started with the oldest binary (the way back).
#
#   OWNER_BASE_URL=postgres://conta_verde:conta_verde@127.0.0.1:5432 \
#   APP_BASE_URL=postgres://conta_verde_app:conta_verde_app@127.0.0.1:5432 \
#   scripts/ops/rehearse-upgrade.sh 623cbdc HEAD            # one upgrade
#   scripts/ops/rehearse-upgrade.sh v1.0.0 v1.0.1 v1.1.0    # two consecutive upgrades
#
# It builds every ref (a release build takes minutes; the target directory is shared), needs psql, pg_dump and pg_restore of PostgreSQL 17
# (scripts/ops/docker-pg-tools.sh) and the role conta_verde_app, and makes and drops databases named cv_upgrade_*. It does NOT rehearse
# a real host, a proxy, a certificate or a managed database.
set -euo pipefail

[ "$#" -ge 2 ] || { echo "usage: rehearse-upgrade.sh OLDEST_REF [MIDDLE_REF...] NEWEST_REF" >&2; exit 2; }
: "${OWNER_BASE_URL:?set OWNER_BASE_URL}"
: "${APP_BASE_URL:?set APP_BASE_URL}"
API_PORT="${API_PORT:-3191}"
# The commit that made the server serve requests as a role of its own (ADR 0093): releases before it run as the owner.
RLS_COMMIT="${RLS_COMMIT:-fabf279}"
here="$(cd "$(dirname "$0")" && pwd)"
repo="$(git -C "$here" rev-parse --show-toplevel)"
export CARGO_TARGET_DIR="${CARGO_TARGET_DIR:-$repo/target}"
work="$(mktemp -d)"
suffix="$(date +%s)"
db="cv_upgrade_$suffix"
db_back="cv_upgrade_back_$suffix"
server_pid=""
refs=("$@")

stop_server() {
    if [ -n "$server_pid" ]; then
        kill "$server_pid" 2>/dev/null || true
        wait "$server_pid" 2>/dev/null || true
        server_pid=""
    fi
}
cleanup() {
    stop_server
    for ref_dir in "$work"/src-*; do
        [ -d "$ref_dir" ] && git -C "$repo" worktree remove --force "$ref_dir" 2>/dev/null || true
    done
    psql -X -q "$OWNER_BASE_URL/postgres" -c "DROP DATABASE IF EXISTS $db WITH (FORCE)" -c "DROP DATABASE IF EXISTS $db_back WITH (FORCE)" >/dev/null 2>&1 || true
    rm -rf "$work"
}
trap cleanup EXIT

build() { # index, ref -> $work/bin-<index>
    local index="$1" ref="$2"
    git -C "$repo" worktree add --detach "$work/src-$index" "$ref" >/dev/null 2>&1
    (cd "$work/src-$index" && cargo build -q -p conta-verde-api)
    cp "$CARGO_TARGET_DIR/debug/conta-verde" "$work/bin-$index"
    # The checks of a release are the checks of the code being rehearsed, not of the old ref.
    if git -C "$repo" merge-base --is-ancestor "$RLS_COMMIT" "$ref" 2>/dev/null; then echo role > "$work/mode-$index"; else echo owner > "$work/mode-$index"; fi
}

start_server() { # index, database, files directory; leaves the server running
    local index="$1" database="$2" files="$3" runtime_url migrate_url
    if [ "$(cat "$work/mode-$index")" = role ]; then runtime_url="$APP_BASE_URL/$database"; else runtime_url="$OWNER_BASE_URL/$database"; fi
    migrate_url="$OWNER_BASE_URL/$database"
    DATABASE_URL="$runtime_url" DATABASE_MIGRATE_URL="$migrate_url" ATTACHMENTS_DIR="$files" BIND_ADDR="127.0.0.1:$API_PORT" \
        CNPJ_LOOKUP=off PUBLIC_URL=http://localhost:5173 "$work/bin-$index" > "$work/server-$index.log" 2>&1 &
    server_pid=$!
    for _ in $(seq 1 60); do
        curl -sf "http://127.0.0.1:$API_PORT/health" >/dev/null && return 0
        kill -0 "$server_pid" 2>/dev/null || { echo "the server of ${refs[$index]} stopped:" >&2; tail -5 "$work/server-$index.log" >&2; return 1; }
        sleep 1
    done
    echo "the server of ${refs[$index]} did not answer" >&2
    return 1
}

counts() { "$here/fingerprint.sh" "$OWNER_BASE_URL/$1" | grep -v '^policies|' | grep -v '^schema_version|' | cut -d'|' -f1,2; }

echo "== building ${#refs[@]} releases"
for index in "${!refs[@]}"; do
    echo "  ${refs[$index]}"
    build "$index" "${refs[$index]}"
done

echo "== the installation, made with ${refs[0]}"
psql -X -q "$OWNER_BASE_URL/postgres" -c "CREATE DATABASE $db"
mkdir -p "$work/files"
start_server 0 "$db" "$work/files"
python3 "$here/check_books.py" seed "http://127.0.0.1:$API_PORT" "$work/state.json"
python3 "$here/check_books.py" verify "http://127.0.0.1:$API_PORT" "$work/state.json" >/dev/null
stop_server
counts "$db" > "$work/counts-0.txt"

echo "== the backup before the upgrade"
BACKUP_DATABASE_URL="$OWNER_BASE_URL/$db" ATTACHMENTS_DIR="$work/files" BACKUP_DIR="$work/backups" "$here/backup.sh"
backup="$(ls -d "$work"/backups/conta-verde-* | tail -1)"

for index in $(seq 1 $((${#refs[@]} - 1))); do
    echo "== in place: ${refs[$((index - 1))]} -> ${refs[$index]}"
    start_server "$index" "$db" "$work/files"
    version="$(curl -s "http://127.0.0.1:$API_PORT/health")"
    echo "  the server says $version"
    python3 "$here/check_books.py" verify "http://127.0.0.1:$API_PORT" "$work/state.json"
    stop_server
    counts "$db" > "$work/counts-$index.txt"
    diff <(grep -F -f <(cut -d'|' -f1 "$work/counts-0.txt" | sed 's/$/|/') "$work/counts-$index.txt") "$work/counts-0.txt" \
        && echo "  ok  every table of ${refs[0]} has the rows it had"
done

echo "== the oldest release refuses the upgraded database"
if start_server 0 "$db" "$work/files" 2>/dev/null; then
    stop_server
    echo "FAILED: ${refs[0]} started on a database that a later release migrated" >&2
    exit 1
fi
stop_server
grep -qi "migration" "$work/server-0.log" && echo "  ok  ${refs[0]} stops with: $(grep -i migration "$work/server-0.log" | tail -1 | sed "s/\x1b\[[0-9;]*m//g" | cut -c1-200)"

echo "== the way back: the backup restored, and ${refs[0]} on it"
psql -X -q "$OWNER_BASE_URL/postgres" -c "CREATE DATABASE $db_back"
RESTORE_DATABASE_URL="$OWNER_BASE_URL/$db_back" ATTACHMENTS_DIR="$work/files-back" "$here/restore.sh" "$backup"
start_server 0 "$db_back" "$work/files-back"
python3 "$here/check_books.py" verify "http://127.0.0.1:$API_PORT" "$work/state.json"
stop_server
echo "upgrade rehearsal ok"
