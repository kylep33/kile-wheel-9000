#Requires -Version 5.1
<#
.SYNOPSIS
    Installs or updates kile-wheel-9000. Safe to re-run.
.DESCRIPTION
    Installs missing prerequisites, builds release, replaces the installed binary,
    and registers an elevated logon task. Never touches an existing user config.
.PARAMETER PrereqsOnly
    Install the toolchain and exit. Nothing is built or installed.
.PARAMETER SkipPrereqs
    Assume the toolchain is present. Fails fast if it is not.
.PARAMETER NoStart
    Install but do not launch.
#>
[CmdletBinding()]
param(
    [switch]$PrereqsOnly,
    [switch]$SkipPrereqs,
    [switch]$NoStart
)

Set-StrictMode -Version 3.0
$ErrorActionPreference = 'Stop'

. (Join-Path $PSScriptRoot 'app-common.ps1')

$RustupPackage = 'Rustlang.Rustup'
$BuildToolsPackage = 'Microsoft.VisualStudio.2022.BuildTools'
$VcToolsWorkload = 'Microsoft.VisualStudio.Workload.VCTools'
$VcToolsComponent = 'Microsoft.VisualStudio.Component.VC.Tools.x86.x64'
$NoExecutionLimit = New-TimeSpan -Seconds 0
$VsWhere = Join-Path ${env:ProgramFiles(x86)} 'Microsoft Visual Studio\Installer\vswhere.exe'
$Manifest = Join-Path $PSScriptRoot 'Cargo.toml'

function Get-ForwardedSwitches {
    $forwarded = @()
    if ($PrereqsOnly) { $forwarded += '-PrereqsOnly' }
    if ($SkipPrereqs) { $forwarded += '-SkipPrereqs' }
    if ($NoStart) { $forwarded += '-NoStart' }
    return $forwarded
}

function Assert-Winget {
    if (Find-Executable 'winget') { return }
    throw "winget not found, so prerequisites cannot be installed automatically. " +
        "Install App Installer from the Microsoft Store, or install Rust and the " +
        "Visual Studio C++ build tools by hand, then re-run with -SkipPrereqs."
}

function Install-WingetPackage($packageId, $overrideArgs) {
    $arguments = @(
        'install', '--id', $packageId, '--exact', '--source', 'winget',
        '--accept-package-agreements', '--accept-source-agreements', '--disable-interactivity'
    )
    if ($overrideArgs) { $arguments += @('--override', $overrideArgs) }

    & winget @arguments
    if ($LASTEXITCODE -ne 0) {
        throw "winget failed to install $packageId (exit $LASTEXITCODE). " +
            "Run 'winget install --id $packageId' manually to see the full error, then re-run this script."
    }
}

function Test-MsvcToolchain {
    if (-not (Test-Path $VsWhere)) { return $false }
    $found = & $VsWhere -latest -products * -requires $VcToolsComponent -property installationPath
    return [bool]$found
}

function Install-MsvcToolchain {
    if (Test-MsvcToolchain) {
        Write-Detail 'msvc build tools present'
        return
    }

    Write-Step 'installing visual studio c++ build tools (several GB, this takes a while)'
    Assert-Winget
    Install-WingetPackage $BuildToolsPackage "--quiet --wait --norestart --add $VcToolsWorkload --includeRecommended"

    if (Test-MsvcToolchain) { return }
    throw "c++ build tools still missing after install, so rust has no linker. " +
        "Open the Visual Studio Installer, add the 'Desktop development with C++' workload, then re-run."
}

function Resolve-Cargo {
    $cargo = Find-Executable 'cargo'
    if ($cargo) { return $cargo }

    $local = Join-Path $CargoBin 'cargo.exe'
    if (Test-Path $local) {
        $env:PATH = "$CargoBin;$env:PATH"
        return $local
    }
    return $null
}

function Install-RustToolchain {
    if (Resolve-Cargo) {
        Write-Detail "rust present: $(& cargo --version)"
        return
    }

    Write-Step 'installing rust via rustup'
    Assert-Winget
    Install-WingetPackage $RustupPackage $null

    if (Resolve-Cargo) { return }
    throw "cargo not found after installing rustup, expected it in $CargoBin. " +
        "Open a new shell, confirm 'cargo --version' works, then re-run with -SkipPrereqs."
}

function Install-RustTarget {
    $installed = & rustup target list --installed
    if ($installed -contains $RustTarget) { return }

    Write-Step "adding rust target $RustTarget"
    & rustup target add $RustTarget
    if ($LASTEXITCODE -ne 0) { throw "rustup could not add target $RustTarget (exit $LASTEXITCODE)." }
}

function Install-Prereqs {
    Write-Step 'installing prereqs if not present'
    Install-MsvcToolchain
    Install-RustToolchain
    Install-RustTarget
}

function Assert-Prereqs {
    if (-not (Resolve-Cargo)) {
        throw "cargo not found and -SkipPrereqs was given. Drop -SkipPrereqs to install the toolchain."
    }
    if (-not (Test-MsvcToolchain)) {
        throw "msvc c++ build tools not found and -SkipPrereqs was given. Drop -SkipPrereqs to install them."
    }
}

function Get-WheelVersion {
    $metadata = & cargo metadata --no-deps --format-version 1 --manifest-path $Manifest | ConvertFrom-Json
    $package = $metadata.packages | Where-Object { $_.name -eq $WheelPackage }
    if (-not $package) { throw "package '$WheelPackage' not found in the cargo workspace at $Manifest." }
    return $package.version
}

function Invoke-ReleaseBuild {
    Write-Step 'building release'
    & cargo build --release --locked --target $RustTarget --manifest-path $Manifest
    if ($LASTEXITCODE -ne 0) { throw "cargo build failed (exit $LASTEXITCODE). Fix the build errors above and re-run." }

    $built = Join-Path $PSScriptRoot "target\$RustTarget\release"
    foreach ($name in @("$AppName.exe", $SettingsExe)) {
        $path = Join-Path $built $name
        if (-not (Test-Path $path)) { throw "build reported success but $path is missing." }
    }
    return $built
}

# the settings window sits beside the wheel, which is where the wheel looks for it
function Install-Binaries($builtDir) {
    Write-Step "installing to $InstallDir"
    New-Item -ItemType Directory -Path $InstallDir -Force | Out-Null

    foreach ($name in @("$AppName.exe", $SettingsExe)) {
        Copy-Item -Path (Join-Path $builtDir $name) -Destination (Join-Path $InstallDir $name) -Force
        Write-Detail "  $name"
    }
}

# config.toml is written by the app on first run so the schema lives in one place
function Install-ConfigDirs {
    foreach ($leaf in @('icons', 'logs')) {
        New-Item -ItemType Directory -Path (Join-Path $ConfigDir $leaf) -Force | Out-Null
    }
    Write-Detail "config dir: $ConfigDir"
}

# a Run key entry cannot start an elevated app without a UAC prompt every logon
function Register-StartupTask($exePath) {
    Write-Step 'registering elevated logon task'
    $action = New-ScheduledTaskAction -Execute $exePath
    $trigger = New-ScheduledTaskTrigger -AtLogOn -User $env:USERNAME
    $principal = New-ScheduledTaskPrincipal -UserId $env:USERNAME -LogonType Interactive -RunLevel Highest
    $settings = New-ScheduledTaskSettingsSet -AllowStartIfOnBatteries -DontStopIfGoingOnBatteries `
        -ExecutionTimeLimit $NoExecutionLimit -MultipleInstances IgnoreNew

    Register-ScheduledTask -TaskName $TaskName -Action $action -Trigger $trigger `
        -Principal $principal -Settings $settings -Force | Out-Null
}

function Write-Summary($version, $exePath) {
    Write-Host ''
    Write-Host "$DisplayName $version installed" -ForegroundColor Green
    Write-Detail "binary:   $exePath"
    Write-Detail "settings: $(Join-Path $InstallDir $SettingsExe)"
    Write-Detail "config:   $ConfigDir"
    Write-Detail "startup:  scheduled task '$TaskName' (highest privileges, at logon)"
    Write-Detail "hotkey:   $DefaultHotkey opens the wheel"
}

if (-not (Test-Elevated)) { Invoke-SelfElevate $PSScriptRoot 'install.ps1' (Get-ForwardedSwitches) }

if ($SkipPrereqs) { Assert-Prereqs } else { Install-Prereqs }

if ($PrereqsOnly) {
    Write-Host ''
    Write-Host 'prereqs ready' -ForegroundColor Green
    exit 0
}

$version = Get-WheelVersion

# stop first: a dev run launched straight out of target\ holds a lock on the very file
# the linker is about to write
Stop-AppInstance

$binaries = Invoke-ReleaseBuild
Install-Binaries $binaries
Install-ConfigDirs

$exePath = Join-Path $InstallDir "$AppName.exe"
Register-StartupTask $exePath
if (-not $NoStart) { Start-Process -FilePath $exePath }

Write-Summary $version $exePath
