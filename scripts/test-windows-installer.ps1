# Exercise the actual NSIS installer without launching the desktop or real bots.
[CmdletBinding()]
param([Parameter(Mandatory = $true)][string]$Installer)
$ErrorActionPreference = 'Stop'
$repoRoot = Split-Path -Parent $PSScriptRoot
$installerPath = (Resolve-Path -LiteralPath $Installer).Path
$uninstallKey = 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Uninstall\Gravity'
$productKey = 'HKCU:\Software\mikolajczuk\Gravity'
if ((Test-Path -LiteralPath $uninstallKey) -or (Test-Path -LiteralPath $productKey)) {
    throw 'Installer smoke test requires a clean Gravity installation registry.'
}
$testRoot = Join-Path ([IO.Path]::GetTempPath()) ('gravity-installer-smoke-' + [guid]::NewGuid())
$installDir = Join-Path $testRoot 'app with spaces'
$stateDir = Join-Path $testRoot 'daemon state'
$daemon = Join-Path $installDir 'gravityd.exe'
$uninstaller = Join-Path $installDir 'uninstall.exe'
$previousGravityHome = $env:GRAVITY_HOME
$expectedDaemon = Join-Path $repoRoot 'apps/desktop/src-tauri/binaries/gravityd-x86_64-pc-windows-msvc.exe'
$expectedHash = (Get-FileHash -LiteralPath $expectedDaemon -Algorithm SHA256).Hash

function Wait-For([scriptblock]$Condition, [string]$Failure) {
    $deadline = [DateTime]::UtcNow.AddSeconds(45)
    while (!( & $Condition )) {
        if ([DateTime]::UtcNow -ge $deadline) { throw $Failure }
        Start-Sleep -Milliseconds 200
    }
}

function Install-App {
    # NSIS requires /D last and unquoted, including a path containing spaces.
    $process = Start-Process -FilePath $installerPath -ArgumentList @('/S', "/D=$installDir") -WindowStyle Hidden -PassThru -Wait
    if ($process.ExitCode -ne 0) { throw "Installer exited $($process.ExitCode)" }
    Wait-For {
        $portFile = Join-Path $stateDir 'gravityd.port'
        if (!(Test-Path -LiteralPath $portFile)) { return $false }
        $port = (Get-Content -LiteralPath $portFile -Raw).Trim()
        try {
            $health = Invoke-RestMethod -Uri "http://127.0.0.1:$port/health" -TimeoutSec 2
            return $health.status -eq 'ok'
        } catch { return $false }
    } 'Installer did not start a healthy bundled daemon.'
    foreach ($binary in @($daemon, (Join-Path $stateDir 'bin/gravityd.exe'))) {
        if ((Get-FileHash -LiteralPath $binary -Algorithm SHA256).Hash -ne $expectedHash) {
            throw 'Installed daemon differs from the installer sidecar.'
        }
    }
    if (!(Test-Path -LiteralPath (Join-Path $stateDir 'gravityd-task.xml'))) {
        throw 'Installer did not register the daemon task.'
    }
    $app = Join-Path $installDir 'gravity-desktop.exe'
    if (Get-CimInstance Win32_Process | Where-Object { $_.ExecutablePath -eq $app }) {
        throw 'The silent install must start the daemon without launching the desktop.'
    }
}

function Uninstall-App {
    $process = Start-Process -FilePath $uninstaller -ArgumentList '/S' -WindowStyle Hidden -PassThru -Wait
    if ($process.ExitCode -ne 0) { throw "Uninstaller exited $($process.ExitCode)" }
    # NSIS may relaunch the uninstaller from a temporary directory.
    Wait-For {
        return !(Test-Path -LiteralPath $uninstallKey) -and
            !(Test-Path -LiteralPath (Join-Path $installDir 'gravity-desktop.exe')) -and
            !(Test-Path -LiteralPath (Join-Path $stateDir 'gravityd-task.xml')) -and
            !(Test-Path -LiteralPath (Join-Path $stateDir 'bin/gravityd.exe'))
    } 'Uninstaller left the app or managed daemon installed.'
}

try {
    New-Item -ItemType Directory -Path $stateDir -Force | Out-Null
    $env:GRAVITY_HOME = $stateDir
    $config = Join-Path $stateDir 'gravityd.toml'
    $configText = "port = 49777`nruntime = `"double`"`nmax_bots_per_project = 7`n"
    [IO.File]::WriteAllText($config, $configText)
    $projectDir = Join-Path $stateDir 'projects/installer-sentinel'
    New-Item -ItemType Directory -Path $projectDir -Force | Out-Null
    $sentinel = Join-Path $projectDir 'keep.txt'
    [IO.File]::WriteAllText($sentinel, 'preserved project data')

    Install-App
    $firstPid = (Get-Content -LiteralPath (Join-Path $stateDir 'gravityd-task.pid') -Raw).Trim()
    Write-Output 'Fresh install started the matching daemon without opening the app.'
    Install-App
    $secondPid = (Get-Content -LiteralPath (Join-Path $stateDir 'gravityd-task.pid') -Raw).Trim()
    if ($firstPid -eq $secondPid) { throw 'Same-version reinstall did not refresh the daemon.' }
    if ((Get-Content -LiteralPath $config -Raw) -ne $configText -or
        (Get-Content -LiteralPath $sentinel -Raw) -ne 'preserved project data') {
        throw 'Reinstall changed saved configuration or project data.'
    }
    Write-Output 'Same-version reinstall refreshed the daemon and preserved data.'
    Uninstall-App
    if (!(Test-Path -LiteralPath $config) -or !(Test-Path -LiteralPath $sentinel)) {
        throw 'Uninstall removed saved configuration or project data.'
    }
    Write-Output 'Desktop uninstall also removed the daemon and preserved data.'

    Install-App
    & $daemon service uninstall
    if ($LASTEXITCODE -ne 0) { throw 'Manual daemon removal failed.' }
    Uninstall-App
    Write-Output 'Desktop uninstall succeeded after the daemon was already removed.'
} finally {
    try {
        if (Test-Path -LiteralPath $uninstaller) { Uninstall-App }
        $marker = Join-Path $stateDir 'gravityd-task.xml'
        if (Test-Path -LiteralPath $marker) {
            & $expectedDaemon service uninstall
            if ($LASTEXITCODE -ne 0) { throw 'Could not clean up the isolated daemon.' }
        }
        if (Test-Path -LiteralPath $productKey) {
            $registeredDir = (Get-Item -LiteralPath $productKey).GetValue('')
            if ($registeredDir -ne $installDir) { throw 'Another Gravity install replaced test registration.' }
            Remove-Item -LiteralPath $productKey -Recurse -Force
        }
        if (Test-Path -LiteralPath $testRoot) {
            $resolved = (Resolve-Path -LiteralPath $testRoot).Path
            $temporaryRoot = [IO.Path]::GetFullPath([IO.Path]::GetTempPath()).TrimEnd('\')
            if (!$resolved.StartsWith($temporaryRoot + '\', [StringComparison]::OrdinalIgnoreCase) -or
                [IO.Path]::GetFileName($resolved) -notlike 'gravity-installer-smoke-*') {
                throw 'Refusing cleanup outside the isolated installer test directory.'
            }
            if (Get-ChildItem -LiteralPath $resolved -Recurse -Force -Attributes ReparsePoint) {
                throw 'Installer test directory contains unexpected links.'
            }
            Remove-Item -LiteralPath $resolved -Recurse -Force
        }
    } finally { $env:GRAVITY_HOME = $previousGravityHome }
}
