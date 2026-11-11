#!/bin/bash
# A fingerprint of the books in a database: for every table its row count and a hash of every row, plus the policies of ADR 0093 and
# the schema version. Two databases with the same fingerprint hold the same data (ADR 0096): the restore rehearsal compares the
# database before a backup with the one restored from it, and the upgrade rehearsal compares before and after an upgrade.
#
#   scripts/ops/fingerprint.sh postgres://conta_verde:...@db:5432/conta_verde > before.txt
#
# What is left out is what a running server rewrites on its own: sessions, the job queue and the webhook delivery log.
set -euo pipefail
url="${1:?usage: fingerprint.sh DATABASE_URL}"
# stdin is closed: a wrapped psql would otherwise read the list of tables the loop below is reading.
psql_q() { psql -X -A -t -q "$url" -c "$1" </dev/null; }

# A table a release adds is in the fingerprint of that release: an upgrade is compared on the tables both have.
only="${FINGERPRINT_ONLY:-}"
while read -r table; do
    [ -z "$table" ] && continue
    if [ -n "$only" ] && ! grep -qx "$table" <<<"$only"; then continue; fi
    psql_q "SELECT '$table', count(*), coalesce(md5(string_agg(t::text, '|' ORDER BY t::text)), '') FROM \"$table\" t"
done < <(psql_q "SELECT tablename FROM pg_tables WHERE schemaname = 'public' AND tablename NOT IN ('sessions', 'jobs', 'webhook_deliveries', '_sqlx_migrations') ORDER BY 1")
psql_q "SELECT 'policies', count(*) FROM pg_policy"
psql_q "SELECT 'schema_version', max(version) FROM _sqlx_migrations"
