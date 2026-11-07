#!/bin/bash
# The platform injects DATABASE_URL for the attached instance (database nous_dev); DATABASE_NAME picks
# the app's own database inside it, keeping host and credentials as injected.
if [ -n "$DATABASE_NAME" ]; then
  DATABASE_URL=$(printf '%s' "$DATABASE_URL" | sed -E "s#/[^/?]+(\?|\$)#/${DATABASE_NAME}\1#")
  export DATABASE_URL
fi
# Keeps nginx up even when the API dies, so the platform can route to the container and its logs show why
# the API stopped; a missing or read-only attachments directory is reported, not fatal.
mkdir -p "$ATTACHMENTS_DIR" || echo "cannot create ATTACHMENTS_DIR=$ATTACHMENTS_DIR" >&2
(conta-verde; echo "conta-verde exited with status $?" >&2) &
exec nginx -g 'daemon off;'
