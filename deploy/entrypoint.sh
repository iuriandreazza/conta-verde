#!/bin/bash
set -e
mkdir -p "$ATTACHMENTS_DIR"
conta-verde &
api=$!
nginx -g 'daemon off;' &
web=$!
# If either dies the platform must restart the container, not leave half of it serving.
trap 'kill $api $web 2>/dev/null' TERM INT
wait -n $api $web
kill $api $web 2>/dev/null
exit 1
