# web-ui-v2/Dockerfile
#
# Esta imagen NO compila la app: empaqueta el `dist/` que ya se construyo antes.
#
# Antes tenia una etapa `node:20-alpine` que corria `npm ci` y `npm run build` adentro. Eso
# duplicaba trabajo: el workflow (.github/workflows/web-ui-v2.yml, paso "Install dependencies,
# test, and build") ya corre exactamente lo mismo en el host, y despues la imagen lo volvia a
# hacer desde cero en una VM mas lenta. Medido en el release webuiv2-v0.3.0: el `npm run build`
# dentro del contenedor tardo 816s (13m36s) contra 5m45s del paso del host, que ademas incluye
# `npm ci` y los tests. Era el 90% del tiempo del paso "Build Docker image".
#
# CONTRATO: quien construya esta imagen tiene que haber corrido `npm run build` en web-ui-v2/
# primero. El workflow lo garantiza por orden de pasos. Para un build local o por
# docker-compose:
#
#     cd web-ui-v2 && npm ci && npm run build && cd ..
#     docker build -f web-ui-v2/Dockerfile -t ifascada/web-ui-v2:dev .
#
# Si falta el dist, el COPY de abajo falla y el build se detiene con un error claro. El riesgo
# que queda es un dist VIEJO: se veria como una imagen que arranca bien pero sirve codigo
# anterior. Por eso el workflow construye siempre inmediatamente antes, en el mismo job.
#
# (`web-ui-v2/dist` esta excluido por .gitignore pero NEGADO en .dockerignore a proposito: es
# la unica salida de build que si entra al contexto.)

FROM nginx:alpine

COPY web-ui-v2/dist /usr/share/nginx/html

# Falla temprano y con un mensaje entendible si el dist existe pero esta vacio o incompleto,
# en vez de servir un 404 en produccion.
RUN test -s /usr/share/nginx/html/index.html \
  || (echo "ERROR: web-ui-v2/dist no contiene un index.html valido. Corre 'npm run build' antes." && exit 1)

COPY web-ui-v2/nginx.conf.template /etc/nginx/templates/nginx.conf.template
COPY web-ui-v2/docker-entrypoint.sh /docker-entrypoint.sh
RUN chmod +x /docker-entrypoint.sh
ENV CENTRAL_API_UPSTREAM=http://127.0.0.1:8088
EXPOSE 3002
ENTRYPOINT ["/docker-entrypoint.sh"]
