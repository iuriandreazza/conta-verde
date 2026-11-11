#!/bin/bash
# Makes the role the application serves requests as, with a password from the environment (ADR 0093, ADR 0096): the image's
# first start of a database runs it from /docker-entrypoint-initdb.d. For a database that already exists, run it by hand as
# the owner: APP_DB_PASSWORD=... POSTGRES_USER=... POSTGRES_DB=... deploy/postgres/create-app-role.sh
# It can be run again: an existing role keeps its password unless FORCE_APP_DB_PASSWORD=1.
set -euo pipefail
: "${APP_DB_PASSWORD:?set APP_DB_PASSWORD}"
psql -v ON_ERROR_STOP=1 --username "${POSTGRES_USER:-conta_verde}" --dbname "${POSTGRES_DB:-conta_verde}" \
    -v app_password="$APP_DB_PASSWORD" -v force="${FORCE_APP_DB_PASSWORD:-0}" <<'SQL'
SELECT NOT EXISTS (SELECT 1 FROM pg_roles WHERE rolname = 'conta_verde_app') AS missing \gset
\if :missing
    CREATE ROLE conta_verde_app LOGIN PASSWORD :'app_password' NOSUPERUSER NOBYPASSRLS NOCREATEDB NOCREATEROLE;
\elif :force
    ALTER ROLE conta_verde_app PASSWORD :'app_password';
\endif
SQL
