# web-ui/Dockerfile
FROM node:20-alpine AS builder
WORKDIR /app
COPY web-ui/package.json web-ui/package-lock.json ./
RUN npm ci
COPY web-ui/ .
# next.config.mjs's rewrites() (which uses CENTRAL_API_UPSTREAM) is evaluated by `next build`
# and baked into .next/routes-manifest.json -- NOT re-read at container start. Setting this
# only as a docker-compose runtime env var (as production's .env does) has no effect at all;
# every /api/* and /health/* proxy call silently falls back to the http://127.0.0.1:8088
# default (the container's own loopback, where nothing listens) no matter what env vars the
# container is actually started with. Found in production: web-ui:1.0.3's proxy returned
# ECONNREFUSED 127.0.0.1:8088 for every API call despite the correct upstream being set in
# docker-compose.yml -- rolled back to 1.0.2 while root-causing this.
ARG CENTRAL_API_UPSTREAM=http://127.0.0.1:8088
ENV CENTRAL_API_UPSTREAM=$CENTRAL_API_UPSTREAM
RUN npm run build

FROM node:20-alpine
WORKDIR /app
ENV NODE_ENV=production
ENV NEXT_TELEMETRY_DISABLED=1
ENV PORT=3001
COPY web-ui/package.json web-ui/package-lock.json ./
RUN npm ci --omit=dev
COPY --from=builder /app/.next ./.next
COPY --from=builder /app/next.config.mjs ./next.config.mjs
EXPOSE 3001
CMD ["npm", "run", "start", "--", "-p", "3001"]
