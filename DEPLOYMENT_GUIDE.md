# IFA SCADA - Guía de Despliegue en Producción

Esta guía detalla los pasos para instalar y configurar el sistema IFA SCADA en un entorno productivo.

## Requisitos Previos

- **PostgreSQL 18:** Base de datos central.
- **MQTT Broker:** Mosquitto (v2.0+ recomendado).
- **Entorno de Ejecución:** Docker y Docker Compose (recomendado) o binarios nativos (Rust 2024).

---

## Opción 1: Despliegue con Docker (Recomendado)

Esta es la forma más rápida y segura de desplegar el sistema completo (Base de Datos, MQTT y Servidor Central).

### 1. Configuración de Entorno
Crea un archivo `.env` en la raíz con las credenciales de producción:

```env
DATABASE_URL=postgres://admin:tu_password_seguro@db:5432/scada
RUST_LOG=info
```

### 2. Iniciar Servicios
Ejecuta el siguiente comando para construir e iniciar el sistema:

```bash
docker-compose up -d --build
```

Esto iniciará:
- **ifascada-postgres:** Base de datos en el puerto 5432.
- **ifascada-mqtt:** Broker MQTT en el puerto 1883.
- **ifascada-central:** Servidor de API y Dashboard en el puerto 3000.

### 3. Acceder al Dashboard
Abre tu navegador en `http://localhost:3000`. El servidor central ahora sirve automáticamente el Dashboard.

---

## Opción 2: Despliegue Manual (Native)

### 1. Base de Datos (PostgreSQL 18)
1. Instala PostgreSQL 18.
2. Crea la base de datos: `CREATE DATABASE scada;`.
3. Aplica las migraciones:
   ```bash
   psql -U postgres -d scada -f migrations/20260210_001_create_tags_schema.sql
   psql -U postgres -d scada -f migrations/20260216_001_create_reports_schema.sql
   psql -U postgres -d scada -f migrations/202602161245_flexible_report_items.sql
   ```

### 2. Dashboard (Frontend)
1. Ve a la carpeta `web-dashboard`.
2. Instala dependencias: `npm install`.
3. Construye para producción: `npm run build`.
4. Copia el contenido de `dist/web-dashboard/browser` a una carpeta llamada `static` en la raíz donde ejecutarás el servidor central.

### 3. Servidor Central (Backend)
1. Construye el binario: `cargo build --release --bin central-server`.
2. Ejecuta el servidor:
   ```bash
   ./target/release/central-server --api-port 3000 --mqtt-host localhost
   ```

---

## Despliegue del Edge Agent

El Edge Agent debe instalarse en la máquina conectada a los instrumentos físicos (PLC, balanzas, etc.).

1. Construye el binario: `cargo build --release --bin edge-agent`.
2. Configura los tags en `config/default.json` o mediante la base de datos local SQLite.
3. Ejecuta el agente apuntando al Servidor Central (vía MQTT):
   ```bash
   ./edge-agent --agent-id planta-1 --mqtt-host IP_DEL_SERVIDOR_CENTRAL
   ```

---

## Consideraciones de Seguridad
- **Passwords:** Cambia `password` en el `docker-compose.yml` antes de desplegar.
- **Firewall:** Asegúrate de que los puertos 3000 (API) y 1883 (MQTT) estén abiertos solo para las IPs autorizadas.
- **Backups:** Configura backups automáticos para el volumen `postgres_data`.
