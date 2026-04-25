param(
    [string]$TaskName = 'Screener Local Daemon'
)

$ErrorActionPreference = 'Stop'

Unregister-ScheduledTask -TaskName $TaskName -Confirm:$false
Write-Host "Removed startup task '$TaskName'."
