FROM node:20-alpine AS deps
WORKDIR /app
COPY web-ui/package.json web-ui/package-lock.json ./
RUN npm ci

FROM node:20-alpine AS builder
WORKDIR /app
ENV NEXT_TELEMETRY_DISABLED=1
ARG CENTRAL_API_UPSTREAM=http://ifascada-central-server:8088
ENV CENTRAL_API_UPSTREAM=${CENTRAL_API_UPSTREAM}
COPY --from=deps /app/node_modules ./node_modules
COPY web-ui ./
RUN npm run build

FROM node:20-alpine AS runner
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
