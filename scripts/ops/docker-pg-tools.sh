#!/bin/bash
# Makes psql, pg_dump and pg_restore of PostgreSQL 17 available without installing them: small wrappers that run the tools of the
# postgres:17 image on the host's network. A dump must be made by a client at least as new as the server.
#
#   scripts/ops/docker-pg-tools.sh /tmp/pg-tools && export PATH=/tmp/pg-tools:$PATH
set -euo pipefail
dir="${1:?usage: docker-pg-tools.sh DIRECTORY}"
mkdir -p "$dir"
for tool in psql pg_dump pg_restore; do
    cat > "$dir/$tool" <<EOF
#!/bin/sh
exec docker run --rm -i --network host postgres:17 $tool "\$@"
EOF
    chmod +x "$dir/$tool"
done
echo "wrappers in $dir"
