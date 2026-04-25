param(
    [string]$EnvFile = (Join-Path $PSScriptRoot 'local-daemon.env'),
    [string]$Executable = (Join-Path $PSScriptRoot 'local-daemon.exe')
)

$ErrorActionPreference = 'Stop'

if (-not (Test-Path -LiteralPath $Executable)) {
    throw "local-daemon.exe was not found at '$Executable'."
}

if (-not (Test-Path -LiteralPath $EnvFile)) {
    throw "Env file was not found at '$EnvFile'. Copy local-daemon.env.example to local-daemon.env and fill in the values."
}

Get-Content -LiteralPath $EnvFile | ForEach-Object {
    $line = $_.Trim()

    if (-not $line -or $line.StartsWith('#')) {
        return
    }

    $parts = $line -split '=', 2
    if ($parts.Count -ne 2) {
        throw "Invalid env line: '$line'"
    }

    [System.Environment]::SetEnvironmentVariable($parts[0].Trim(), $parts[1].Trim(), 'Process')
}

Start-Process -FilePath $Executable -WorkingDirectory $PSScriptRoot -WindowStyle Hidden
