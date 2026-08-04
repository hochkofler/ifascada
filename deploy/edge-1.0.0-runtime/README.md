# edge-1.0.0-runtime

Paquete para instalar `edge-agent` en Windows sin Docker. Modo recomendado: tarea programada (`Scheduled Task`) para ejecucion al iniciar el equipo.

## Contenido
1. `bin/edge-agent.exe`
2. `config/bootstrap.example.json`
3. `config/edge.env.example`
4. `scripts/install-edge.ps1`
5. `scripts/update-edge-endpoints.ps1`
6. `scripts/uninstall-edge.ps1`

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

## Desinstalacion
```powershell
powershell -ExecutionPolicy Bypass -File .\scripts\uninstall-edge.ps1
```

Para borrar tambien datos locales:
```powershell
powershell -ExecutionPolicy Bypass -File .\scripts\uninstall-edge.ps1 -RemoveData
```
