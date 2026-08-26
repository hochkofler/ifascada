# web-ui-v2/Dockerfile
FROM node:20-alpine AS builder
WORKDIR /app
COPY web-ui-v2/package.json web-ui-v2/package-lock.json ./
RUN npm ci
COPY web-ui-v2/ .
RUN npm run build

FROM nginx:alpine
COPY --from=builder /app/dist /usr/share/nginx/html
COPY web-ui-v2/nginx.conf.template /etc/nginx/templates/nginx.conf.template
COPY web-ui-v2/docker-entrypoint.sh /docker-entrypoint.sh
RUN chmod +x /docker-entrypoint.sh
ENV CENTRAL_API_UPSTREAM=http://127.0.0.1:8088
EXPOSE 3002
ENTRYPOINT ["/docker-entrypoint.sh"]
