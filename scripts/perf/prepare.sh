#!/bin/bash
# Makes the database of the performance baseline (ADR 0097, docs/performance/baseline.md): a fresh database migrated by the server
# itself (so it has the schema, the policies and the grants a real one has), a registered user that owns the large company, and the
# seed of scripts/perf/seed.sql at the given scale.
#
#   OWNER_BASE_URL=postgres://conta_verde:conta_verde@127.0.0.1:5432 \
#   APP_BASE_URL=postgres://conta_verde_app:conta_verde_app@127.0.0.1:5432 \
#   SCALE=1 DB=perf scripts/perf/prepare.sh
#
# It drops and makes the database named by DB (default `perf`). It needs psql of PostgreSQL 17 (scripts/ops/docker-pg-tools.sh), curl and
# a built API (cargo build -p conta-verde-api). The user is perf@example.test with the password in PERF_PASSWORD (default below): a
# throwaway account of a throwaway database.
set -euo pipefail
: "${OWNER_BASE_URL:?set OWNER_BASE_URL}"
: "${APP_BASE_URL:?set APP_BASE_URL}"
DB="${DB:-perf}"
SCALE="${SCALE:-1}"
ACCOUNTS="${ACCOUNTS:-8}"
PERF_PASSWORD="${PERF_PASSWORD:-perf-password-1234}"
API_BIN="${API_BIN:-target/debug/conta-verde}"
here="$(cd "$(dirname "$0")" && pwd)"
port="${API_PORT:-3192}"

psql -X -q "$OWNER_BASE_URL/postgres" -c "DROP DATABASE IF EXISTS $DB WITH (FORCE)" -c "CREATE DATABASE $DB"
DATABASE_URL="$APP_BASE_URL/$DB" DATABASE_MIGRATE_URL="$OWNER_BASE_URL/$DB" BIND_ADDR="127.0.0.1:$port" CNPJ_LOOKUP=off \
    ATTACHMENTS_DIR="$(mktemp -d)" "$API_BIN" > /dev/null 2>&1 &
server=$!
trap 'kill "$server" 2>/dev/null || true' EXIT
for _ in $(seq 1 60); do curl -sf "http://127.0.0.1:$port/health" > /dev/null && break; sleep 1; done
curl -sf -X POST -H 'content-type: application/json' \
    -d "{\"name\":\"Perf\",\"email\":\"perf@example.test\",\"password\":\"$PERF_PASSWORD\",\"company_name\":\"Perf starter\"}" \
    "http://127.0.0.1:$port/auth/register" > /dev/null
kill "$server"
wait "$server" 2>/dev/null || true
trap - EXIT

user_id="$(psql -X -A -t "$OWNER_BASE_URL/$DB" -c "SELECT id FROM users WHERE email = 'perf@example.test'" < /dev/null)"
started="$(date +%s)"
psql -X -q -v user_id="$user_id" -v scale="$SCALE" -v accounts="$ACCOUNTS" -f - "$OWNER_BASE_URL/$DB" < "$here/seed.sql"
echo "seeded $DB at scale $SCALE with $ACCOUNTS bank accounts in $(( $(date +%s) - started )) s"
psql -X -A -t "$OWNER_BASE_URL/$DB" -c "SELECT relname || ' ' || n_live_tup FROM pg_stat_user_tables WHERE n_live_tup > 10000 ORDER BY n_live_tup DESC" < /dev/null
psql -X -A -t "$OWNER_BASE_URL/$DB" -c "SELECT 'database size ' || pg_size_pretty(pg_database_size('$DB'))" < /dev/null
