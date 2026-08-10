# edge-runtime (release 1.1.0)

Paquete para instalar `edge-agent` en Windows sin Docker. Modo recomendado: tarea programada (`Scheduled Task`) para ejecucion al iniciar el equipo.

## Contenido
1. `bin/edge-agent.exe`
2. `config/bootstrap.example.json`
3. `config/edge.env.example`
4. `scripts/install-edge.ps1`
5. `scripts/build-edge-package.ps1`
6. `scripts/update-edge.ps1`
7. `scripts/update-edge-endpoints.ps1`
8. `scripts/uninstall-edge.ps1`
9. `release-manifest.json` (generado junto con el binario)

## Generar paquete release

Desde la raiz del repositorio, el siguiente comando compila `edge-agent`, copia el ejecutable a `bin` y genera `release-manifest.json` con el SHA-256 real:

```powershell
powershell -ExecutionPolicy Bypass -File .\deploy\edge-1.0.0-runtime\scripts\build-edge-package.ps1
```

La version se toma de `VERSION`. Tambien puede indicarse explicitamente con `-Version 1.1.0`. El ejecutable y el manifiesto generado son artefactos del paquete y no se versionan en Git.

## Instalacion en destino
1. Copiar carpeta completa a la maquina edge.
2. Abrir PowerShell como Administrador.
3. Ejecutar:

```powershell
powershell -ExecutionPolicy Bypass -File .\scripts\install-edge.ps1 `
  -EdgeId edge-01 `
  -Site plant-a `
  -CentralHost 192.168.103.70 `
  -MqttPort 51883 `
  -CentralApiPort 8088 `
  -InstallMode task `
  -RunAsUser "PLA-LAB-059\user"
```

`InstallMode`:
1. `task` (recomendado): Scheduled Task (sin dependencias externas).
2. `auto`: usa `nssm` si existe, sino Scheduled Task.
3. `nssm`: fuerza servicio NSSM.

`RunAsUser`:
1. `SYSTEM` (default): simple, pero puede fallar contra shares de red.
2. `DOMINIO\usuario` o `MAQUINA\usuario`: recomendado para impresion por `\\servidor\impresora`.

## Reinstalacion limpia (prueba)
```powershell
powershell -ExecutionPolicy Bypass -File .\scripts\uninstall-edge.ps1 -RemoveData
powershell -ExecutionPolicy Bypass -File .\scripts\install-edge.ps1 `
  -EdgeId edge-com-01 `
  -Site plant-a `
  -CentralHost 192.168.103.98 `
  -MqttPort 51883 `
  -CentralApiPort 8088 `
  -InstallMode task `
  -RunAsUser "PLA-LAB-059\user"
```

Opcional (no interactivo), usar `SecureString`:
```powershell
$pwd = Read-Host "Password" -AsSecureString
powershell -ExecutionPolicy Bypass -File .\scripts\install-edge.ps1 `
  -EdgeId edge-com-01 `
  -Site plant-a `
  -CentralHost 192.168.103.98 `
  -MqttPort 51883 `
  -CentralApiPort 8088 `
  -InstallMode task `
  -RunAsUser "PLA-LAB-059\user" `
  -RunAsPassword $pwd
```

## Verificacion
```powershell
Get-Service ifascada-edge
Get-ScheduledTask -TaskName ifascada-edge
Get-ScheduledTaskInfo -TaskName ifascada-edge | Format-List LastRunTime,LastTaskResult
Get-Process edge-agent -ErrorAction SilentlyContinue
Get-Content C:\ProgramData\ifascada\edge\logs\edge.out.log -Wait
Get-Content C:\ProgramData\ifascada\edge\logs\edge.task.log -Tail 100
```

## Cambio de IP/host de central
```powershell
powershell -ExecutionPolicy Bypass -File .\scripts\update-edge-endpoints.ps1 `
  -CentralHost 192.168.103.80 `
  -MqttPort 51883 `
  -CentralApiPort 8088
```

## Actualizacion segura del edge-agent

Copiar el paquete release nuevo a la maquina edge y abrir PowerShell como Administrador. No ejecutar nuevamente `install-edge.ps1`: el actualizador conserva la tarea/servicio y todo el contenido de `C:\ProgramData\ifascada\edge`.

```powershell
powershell -ExecutionPolicy Bypass -File .\scripts\update-edge.ps1 `
  -RuntimeMode auto `
  -ServiceName ifascada-edge `
  -TaskName ifascada-edge `
  -TaskPath "\"
```

Antes de detener el edge, el script valida el formato del manifiesto, la compatibilidad del schema y el SHA-256. Luego:

1. Detiene solamente el servicio o tarea indicado y el proceso que ejecuta exactamente el binario instalado.
2. Guarda un snapshot unico en `C:\Program Files\ifascada\edge\releases\<version>\<snapshot>\`.
3. Sustituye el binario y vuelve a iniciar el mismo runtime.
4. Espera hasta 20 segundos por el nuevo proceso.
5. Si el proceso no aparece, restaura el binario y manifiesto anteriores y reinicia el runtime.

No modifica `edge.env`, `bootstrap.json`, `runtime_config.signed.json`, `mqtt_outbox.db`, recibos ni logs. Ademas rechaza solapamientos o junctions entre las raices de paquete, instalacion y datos. En instalaciones antiguas sin manifiesto, el primer respaldo queda bajo `releases\unknown`.

Para una instalacion que usa explicitamente una tarea programada:

```powershell
powershell -ExecutionPolicy Bypass -File .\scripts\update-edge.ps1 -RuntimeMode task -TaskName ifascada-edge -TaskPath "\"
```

Para NSSM/servicio de Windows:

```powershell
powershell -ExecutionPolicy Bypass -File .\scripts\update-edge.ps1 -RuntimeMode service -ServiceName ifascada-edge
```

## Desinstalacion
```powershell
powershell -ExecutionPolicy Bypass -File .\scripts\uninstall-edge.ps1
```

Para borrar tambien datos locales:
```powershell
powershell -ExecutionPolicy Bypass -File .\scripts\uninstall-edge.ps1 -RemoveData
```
