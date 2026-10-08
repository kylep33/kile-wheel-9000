#Requires -Version 5.1
<#
.SYNOPSIS
    Shared names, paths and helpers for install.ps1 and uninstall.ps1.
#>

Set-StrictMode -Version 3.0

# rename the product here and in core::APP_NAME
$script:AppName = 'kile-wheel-9000'
$script:DisplayName = 'Kile Wheel 9000'
$script:DefaultHotkey = 'Caps Lock'

$script:WheelPackage = 'kile-wheel-9000'
$script:SettingsExe = 'kile-wheel-9000-settings.exe'
$script:RustTarget = 'x86_64-pc-windows-msvc'
$script:TaskName = $script:DisplayName
$script:StopTimeout_s = 10

$script:InstallDir = Join-Path $env:LOCALAPPDATA "Programs\$script:AppName"
$script:ConfigDir = Join-Path $env:APPDATA $script:AppName
$script:CargoBin = Join-Path $env:USERPROFILE '.cargo\bin'

function Write-Step($message) {
    Write-Host "==> $message" -ForegroundColor Cyan
}

function Write-Detail($message) {
    Write-Host "    $message" -ForegroundColor DarkGray
}

function Test-Elevated {
    $identity = [Security.Principal.WindowsIdentity]::GetCurrent()
    return ([Security.Principal.WindowsPrincipal]$identity).IsInRole(
        [Security.Principal.WindowsBuiltInRole]::Administrator)
}

# relaunches the calling script elevated and exits; -NoExit keeps the output readable
function Invoke-SelfElevate($scriptDir, $scriptName, $forwardedArgs) {
    Write-Step 'elevating'
    $script = Join-Path $scriptDir $scriptName
    $arguments = @('-NoExit', '-ExecutionPolicy', 'Bypass', '-File', "`"$script`"") + $forwardedArgs
    Start-Process -FilePath 'powershell.exe' -ArgumentList $arguments -Verb RunAs
    exit 0
}

function Find-Executable($name) {
    $command = Get-Command $name -ErrorAction SilentlyContinue
    if ($command) { return $command.Source }
    return $null
}

# force-kill is fine: the overlay has no main window to close, and config writes are atomic
function Stop-AppInstance {
    $names = @($script:AppName, [IO.Path]::GetFileNameWithoutExtension($script:SettingsExe))
    $running = @(Get-Process -Name $names -ErrorAction SilentlyContinue)
    if ($running.Count -eq 0) { return }

    Write-Step "stopping running $script:DisplayName"
    $running | Stop-Process -Force
    Wait-Process -Id $running.Id -Timeout $script:StopTimeout_s -ErrorAction SilentlyContinue
}
