#!/bin/bash
# The platform injects DATABASE_URL for the attached instance (database nous_dev); DATABASE_NAME picks
# the app's own database inside it, keeping host and credentials as injected.
if [ -n "$DATABASE_NAME" ]; then
  DATABASE_URL=$(printf '%s' "$DATABASE_URL" | sed -E "s#/[^/?]+(\?|\$)#/${DATABASE_NAME}\1#")
  export DATABASE_URL
  if [ -n "$DATABASE_MIGRATE_URL" ]; then
    DATABASE_MIGRATE_URL=$(printf '%s' "$DATABASE_MIGRATE_URL" | sed -E "s#/[^/?]+(\?|\$)#/${DATABASE_NAME}\1#")
    export DATABASE_MIGRATE_URL
  fi
fi
# With one role that owns the schema (the development environment's database) the row-level policies do not bind the server
# (docs/adr/0093-row-level-security.md): it runs without that barrier, and says so, unless a runtime role is configured.
if [ -z "$DATABASE_MIGRATE_URL" ] && [ -z "$ALLOW_PRIVILEGED_DATABASE_ROLE" ]; then
  echo "DATABASE_MIGRATE_URL is not set: running without row-level security (ALLOW_PRIVILEGED_DATABASE_ROLE=true)" >&2
  export ALLOW_PRIVILEGED_DATABASE_ROLE=true
fi
# A missing or read-only attachments directory is reported, not fatal.
mkdir -p "$ATTACHMENTS_DIR" || echo "cannot create ATTACHMENTS_DIR=$ATTACHMENTS_DIR" >&2

nginx -g 'daemon off;' &
web=$!
conta-verde &
api=$!
# A stop signal reaches both, so `docker stop` ends the API cleanly instead of killing it with the container.
trap 'kill -TERM "$web" "$api" 2>/dev/null' TERM INT

wait -n "$web" "$api"
if ! kill -0 "$api" 2>/dev/null; then
  wait "$api"
  echo "conta-verde exited with status $?" >&2
  # The development environment keeps the web server up so the platform can route to the container and the logs show why
  # the API stopped. Anywhere that restarts a container that exits (EXIT_WITH_API=true, the production compose file) a dead
  # API must end the container, or it would sit there answering 502 forever.
  if [ "$EXIT_WITH_API" = "true" ]; then
    kill -TERM "$web" 2>/dev/null
    wait "$web" 2>/dev/null
    exit 1
  fi
  wait "$web"
else
  wait "$web"
  kill -TERM "$api" 2>/dev/null
  wait "$api" 2>/dev/null
fi
