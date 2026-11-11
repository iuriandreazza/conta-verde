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
# This image is the development environment's (the production image has its own entrypoint, docs/operations). It is given one role that owns the schema, which the row-level policies do not bind
# (docs/adr/0093-row-level-security.md): it runs without that barrier, and says so, unless a runtime role is configured.
if [ -z "$DATABASE_MIGRATE_URL" ] && [ -z "$ALLOW_PRIVILEGED_DATABASE_ROLE" ]; then
  echo "DATABASE_MIGRATE_URL is not set: running without row-level security (ALLOW_PRIVILEGED_DATABASE_ROLE=true)" >&2
  export ALLOW_PRIVILEGED_DATABASE_ROLE=true
fi
# Keeps nginx up even when the API dies, so the platform can route to the container and its logs show why
# the API stopped; a missing or read-only attachments directory is reported, not fatal.
mkdir -p "$ATTACHMENTS_DIR" || echo "cannot create ATTACHMENTS_DIR=$ATTACHMENTS_DIR" >&2
(conta-verde; echo "conta-verde exited with status $?" >&2) &
exec nginx -g 'daemon off;'
