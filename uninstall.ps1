#Requires -Version 5.1
<#
.SYNOPSIS
    Removes kile-wheel-9000. Safe to re-run.
.DESCRIPTION
    Stops the app, removes the logon task and the installed binary. The user config
    is kept unless -RemoveConfig is given; without either switch you are asked.
.PARAMETER RemoveConfig
    Delete the config folder without asking.
.PARAMETER KeepConfig
    Keep the config folder without asking.
#>
[CmdletBinding()]
param(
    [switch]$RemoveConfig,
    [switch]$KeepConfig
)

Set-StrictMode -Version 3.0
$ErrorActionPreference = 'Stop'

. (Join-Path $PSScriptRoot 'app-common.ps1')

function Get-ForwardedSwitches {
    $forwarded = @()
    if ($RemoveConfig) { $forwarded += '-RemoveConfig' }
    if ($KeepConfig) { $forwarded += '-KeepConfig' }
    return $forwarded
}

function Unregister-StartupTask {
    if (-not (Get-ScheduledTask -TaskName $TaskName -ErrorAction SilentlyContinue)) { return }

    Write-Step "removing scheduled task '$TaskName'"
    Unregister-ScheduledTask -TaskName $TaskName -Confirm:$false
}

function Remove-InstallDir {
    if (-not (Test-Path $InstallDir)) { return }

    Write-Step "removing $InstallDir"
    Remove-Item -Path $InstallDir -Recurse -Force
}

function Test-ShouldRemoveConfig {
    if ($RemoveConfig) { return $true }
    if ($KeepConfig) { return $false }

    $answer = Read-Host "delete config, icons and logs in $ConfigDir ? [y/N]"
    return $answer -eq 'y'
}

function Remove-ConfigDir {
    if (-not (Test-Path $ConfigDir)) { return }

    if (-not (Test-ShouldRemoveConfig)) {
        Write-Detail "kept config: $ConfigDir"
        return
    }

    Write-Step "removing $ConfigDir"
    Remove-Item -Path $ConfigDir -Recurse -Force
}

if ($RemoveConfig -and $KeepConfig) {
    throw "-RemoveConfig and -KeepConfig conflict. Pass one or neither."
}

if (-not (Test-Elevated)) { Invoke-SelfElevate $PSScriptRoot 'uninstall.ps1' (Get-ForwardedSwitches) }

Stop-AppInstance
Unregister-StartupTask
Remove-InstallDir
Remove-ConfigDir

Write-Host ''
Write-Host "$DisplayName removed" -ForegroundColor Green
