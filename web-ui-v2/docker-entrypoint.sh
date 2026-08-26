#!/bin/sh
# web-ui-v2/docker-entrypoint.sh
set -eu
envsubst '${CENTRAL_API_UPSTREAM}' < /etc/nginx/templates/nginx.conf.template > /etc/nginx/conf.d/default.conf
exec nginx -g "daemon off;"
