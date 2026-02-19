# Guía de Despliegue del Edge Agent (Release)

Para mover el Edge Agent a una nueva ubicación o servidor de producción, se requiere un conjunto mínimo de archivos.

## Estructura de Archivos Necesaria

```text
/mi-instalacion-scada
├── edge-agent.exe          (Binario compilado en release)
├── config/                 (Carpeta de configuración)
│   ├── default.toml        (Configuración inicial)
│   └── last_known.json     (Persistencia de config remota)
└── data/                   (Carpeta de datos - se crea sola)
    ├── {id}_storage.db     (Base de datos de tags)
    └── {id}_buffer.db      (Búfer Store & Forward)
```

## Pasos para el Despliegue

1. **Compilar**: Ejecuta `cargo build --release --bin edge-agent`.
2. **Copiar Binario**: Toma el archivo de `target/release/edge-agent.exe`.
3. **Preparar Carpeta**: Crea una carpeta en el destino y pega el `.exe`.
4. **Configurar**: Crea una carpeta `config` junto al `.exe` y añade un `default.toml` con al menos el `agent_id` y la IP del broker MQTT.
5. **Ejecutar**: Lanza el `.exe`. La carpeta `data` se creará automáticamente.

## Portabilidad

El agente detecta automáticamente si se está ejecutando en un entorno de desarrollo o en producción. En producción, buscará siempre las carpetas `config` y `data` en el mismo directorio donde se encuentre el binario o desde donde se ejecute.

## Solución de Problemas (Windows)

Si al hacer doble clic ocurre un error:
1. **La ventana se quedará abierta**: Hemos añadido una pausa especial. Si el agente falla al arrancar, verás el error en rojo y el mensaje "Presiona Enter para cerrar esta ventana...". esto te permite diagnosticar qué falta (ej. el archivo `default.toml`).
2. **Logs de Rutas**: Al inicio verás exactamente qué carpetas está intentando usar el agente:
   - `📂 Base directory: ...`
   - `📂 Config directory: ...`
   - `📂 Data directory: ...`
3. **Broker MQTT**: Asegúrate de que la IP en `default.toml` sea accesible.
