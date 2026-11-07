#!/bin/bash
# Keeps nginx up even when the API dies, so the platform can route to the container and its logs show why
# the API stopped; a missing or read-only attachments directory is reported, not fatal.
mkdir -p "$ATTACHMENTS_DIR" || echo "cannot create ATTACHMENTS_DIR=$ATTACHMENTS_DIR" >&2
(conta-verde; echo "conta-verde exited with status $?" >&2) &
exec nginx -g 'daemon off;'
