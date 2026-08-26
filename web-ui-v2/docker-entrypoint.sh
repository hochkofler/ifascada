#!/bin/sh
# web-ui-v2/docker-entrypoint.sh
set -eu
# proxy_pass now targets a variable (see nginx.conf.template), and nginx does not
# auto-normalize a trailing slash on a variable target the way it does for a literal
# proxy_pass value -- a trailing slash here would silently collapse every proxied
# request path down to "/" at the backend. Strip it defensively before envsubst runs.
CENTRAL_API_UPSTREAM="${CENTRAL_API_UPSTREAM%/}"
envsubst '${CENTRAL_API_UPSTREAM}' < /etc/nginx/templates/nginx.conf.template > /etc/nginx/conf.d/default.conf
exec nginx -g "daemon off;"
