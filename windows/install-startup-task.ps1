param(
    [string]$TaskName = 'Screener Local Daemon',
    [string]$ScriptPath = (Join-Path $PSScriptRoot 'start-local-daemon.ps1')
)

$ErrorActionPreference = 'Stop'

if (-not (Test-Path -LiteralPath $ScriptPath)) {
    throw "Startup script was not found at '$ScriptPath'."
}

$powerShellPath = Join-Path $env:SystemRoot 'System32\WindowsPowerShell\v1.0\powershell.exe'
if (-not (Test-Path -LiteralPath $powerShellPath)) {
    throw "powershell.exe was not found at '$powerShellPath'."
}

$action = New-ScheduledTaskAction `
    -Execute $powerShellPath `
    -Argument "-NoProfile -ExecutionPolicy Bypass -WindowStyle Hidden -File `"$ScriptPath`""

$trigger = New-ScheduledTaskTrigger -AtLogOn -User $env:USERNAME
$principal = New-ScheduledTaskPrincipal -UserId "$env:USERDOMAIN\$env:USERNAME" -LogonType Interactive -RunLevel Limited
$settings = New-ScheduledTaskSettingsSet `
    -AllowStartIfOnBatteries `
    -DontStopIfGoingOnBatteries `
    -MultipleInstances IgnoreNew `
    -StartWhenAvailable

Register-ScheduledTask `
    -TaskName $TaskName `
    -Action $action `
    -Trigger $trigger `
    -Principal $principal `
    -Settings $settings `
    -Force | Out-Null

Write-Host "Installed startup task '$TaskName'."
