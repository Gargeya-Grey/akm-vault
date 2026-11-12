<#
.SYNOPSIS
Install, upgrade and uninstall test of the AKM setup program, for a disposable GitHub-hosted runner only.

.DESCRIPTION
Used by .github/workflows/native-proof.yml. It installs software, so it refuses to run on any
computer that is not a disposable runner (tools/DisposableRunner.psm1), and it refuses when AKM
state already exists: the install folder %LOCALAPPDATA%\AKM (the same folder as the journal's
parent %LOCALAPPDATA%\akm, because Windows ignores case), the settings folder
%APPDATA%\com.akm.desktop, the WebView2 data folder %LOCALAPPDATA%\com.akm.desktop, the uninstall
registry key, or an AKM Start menu entry. Every refusal comes before the first change.

The steps, each recorded in steps.json and as one PASS or FAIL line:

1. Fake user data: a fake vault made with the built CLI, journal records in the default journal
   folder (no AKM_JOURNAL_ROOT, because this runner is disposable), and a settings file that selects
   the fake vault.
2. Install: the setup program with /S (silent, for the current user). Check the program files,
   the uninstall registry entry and the Start menu entry, and that the installed program is the
   desktop build -DesktopExe, which the preflight checked against SHA256SUMS.txt, with only the
   bundler's NSIS mark (tools/InstallBinding.psm1). So an older program in the setup program fails.
3. Start: the installed akm-desktop.exe through tools/verify_windows_app.ps1 -JournalOnly, with
   that script's own fake vaults.
4. Upgrade: the same setup program again over the install. The program files and the user data
   must be the same afterwards. No earlier release exists, so this is a second install of one
   version over an install with user data, not an upgrade from an older version.
5. Uninstall: uninstall.exe /S. The program files, the registry entry and the shortcuts must be
   gone. The vault, the settings and the journal must be byte-identical to step 1. The journal
   folder %LOCALAPPDATA%\akm\journal is inside the install folder, and the uninstaller removes
   that folder only when it is empty (RMDir without /r), so the folder must hold only `journal`.
   A silent uninstall shows no "Delete app data" box; that box, in an interactive uninstall,
   removes %APPDATA%\com.akm.desktop and %LOCALAPPDATA%\com.akm.desktop, and no test covers it.

The tool deletes nothing itself. The only removal is the AKM uninstaller, on the install that this
tool made.

.EXAMPLE
./tools/test_install.ps1 -Setup out\AKM_0.1.0_x64-setup.exe -DesktopExe out\akm-desktop.exe -CliExe out\akm.exe -Evidence $env:RUNNER_TEMP\evidence\install -DisposableRunner
#>
param(
    [Parameter(Mandatory = $true)][string]$Setup,
    [Parameter(Mandatory = $true)][string]$DesktopExe,
    [Parameter(Mandatory = $true)][string]$CliExe,
    [Parameter(Mandatory = $true)][string]$Evidence,
    [string]$ProofScript,
    [switch]$DisposableRunner
)
$ErrorActionPreference = 'Stop'
Import-Module (Join-Path $PSScriptRoot 'DisposableRunner.psm1') -Force
Assert-DisposableRunner -Tool 'tools/test_install.ps1' -Confirmed:$DisposableRunner
Import-Module (Join-Path $PSScriptRoot 'InstallBinding.psm1') -Force
# Windows PowerShell 5.1 leaves $PSScriptRoot empty in a parameter default, so the default is set here.
if (-not $ProofScript) { $ProofScript = Join-Path $PSScriptRoot 'verify_windows_app.ps1' }

$installDir = Join-Path $env:LOCALAPPDATA 'AKM'
$settingsDir = Join-Path $env:APPDATA 'com.akm.desktop'
$webviewData = Join-Path $env:LOCALAPPDATA 'com.akm.desktop'
$journalDir = Join-Path $env:LOCALAPPDATA 'akm\journal'
$uninstallKey = 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Uninstall\AKM'
$startMenu = Join-Path $env:APPDATA 'Microsoft\Windows\Start Menu\Programs'
$desktopLink = Join-Path ([Environment]::GetFolderPath('Desktop')) 'AKM.lnk'

$existing = @()
foreach ($path in @($installDir, $settingsDir, $webviewData)) {
    if (Test-Path -LiteralPath $path) { $existing += $path }
}
if (Test-Path -LiteralPath $uninstallKey) { $existing += $uninstallKey }
if (Test-Path -LiteralPath $startMenu) {
    $links = @(Get-ChildItem -LiteralPath $startMenu -Recurse -Filter 'AKM.lnk' -ErrorAction SilentlyContinue)
    $existing += $links.FullName
}
if (Test-Path -LiteralPath $desktopLink) { $existing += $desktopLink }
if ($existing.Count) {
    throw "REFUSED: AKM state exists already on this computer: $($existing -join '; '). A test install could change it. Nothing was changed."
}
foreach ($file in @($Setup, $DesktopExe, $CliExe, $ProofScript)) {
    if (-not (Test-Path -LiteralPath $file -PathType Leaf)) { throw "REFUSED: $file does not exist. Nothing was changed." }
}
if (Test-Path -LiteralPath $Evidence) { throw "REFUSED: the evidence folder $Evidence exists already. Nothing was changed." }

New-Item -ItemType Directory -Path $Evidence | Out-Null
$setupFull = (Resolve-Path -LiteralPath $Setup).Path
$desktopFull = (Resolve-Path -LiteralPath $DesktopExe).Path
$cliFull = (Resolve-Path -LiteralPath $CliExe).Path
$work = Join-Path $env:RUNNER_TEMP ('akm-install-test-' + [Guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Path $work | Out-Null
$vault = Join-Path $work 'vault.kdbx'
$settings = Join-Path $settingsDir 'settings.json'
$password = 'fake install test password 0000'
$steps = New-Object System.Collections.Generic.List[object]
$failed = $false

function Step([string]$name, [scriptblock]$body) {
    $started = [DateTime]::UtcNow
    try {
        $detail = (& $body | Out-String).Trim()
        $steps.Add([ordered]@{ step = $name; result = 'PASS'; detail = $detail; utc = $started.ToString('o') })
        Write-Output "PASS: $name$(if ($detail) { " ($detail)" })"
    } catch {
        $steps.Add([ordered]@{ step = $name; result = 'FAIL'; detail = $_.Exception.Message; utc = $started.ToString('o') })
        Write-Output "FAIL: $name ($($_.Exception.Message))"
        $script:failed = $true
        throw
    }
}
function Assert-True([bool]$condition, [string]$message) { if (-not $condition) { throw $message } }

function Invoke-Cli([string[]]$arguments, [string]$stdin) {
    $psi = New-Object System.Diagnostics.ProcessStartInfo
    $psi.FileName = $cliFull
    $psi.Arguments = ($arguments | ForEach-Object { if ($_ -match '[\s"]') { '"' + ($_ -replace '"', '\"') + '"' } else { $_ } }) -join ' '
    $psi.UseShellExecute = $false
    $psi.RedirectStandardInput = $true
    $psi.RedirectStandardOutput = $true
    $psi.RedirectStandardError = $true
    foreach ($name in @($psi.EnvironmentVariables.Keys)) { if ($name -like 'AKM_*') { $psi.EnvironmentVariables.Remove($name) } }
    $psi.EnvironmentVariables['AKM_VAULT'] = $vault
    $psi.EnvironmentVariables['AKM_PASSWORD'] = $password
    $process = [Diagnostics.Process]::Start($psi)
    if ($stdin) { $process.StandardInput.WriteLine($stdin) }
    $process.StandardInput.Close()
    $out = $process.StandardOutput.ReadToEnd()
    $err = $process.StandardError.ReadToEnd()
    $process.WaitForExit()
    if ($process.ExitCode -ne 0) { throw "akm $($arguments[0]) exited $($process.ExitCode): $err" }
    return $out
}

# The user data that an install, a reinstall and an uninstall must leave alone, as path -> SHA-256.
function Get-UserData {
    $files = [ordered]@{}
    foreach ($file in @(Get-Item -LiteralPath $vault, $settings) + @(Get-ChildItem -LiteralPath $journalDir -Recurse -File)) {
        $files[$file.FullName] = (Get-FileHash -LiteralPath $file.FullName -Algorithm SHA256).Hash.ToLowerInvariant()
    }
    return $files
}
function Assert-SameUserData($before) {
    $after = Get-UserData
    $changed = @($before.Keys | Where-Object { $after[$_] -ne $before[$_] }) + @($after.Keys | Where-Object { -not $before.Contains($_) })
    Assert-True ($changed.Count -eq 0) "user data changed: $($changed -join '; ')"
    return "$($before.Count) files unchanged"
}

function Get-Installed {
    $entry = Get-ItemProperty -LiteralPath $uninstallKey
    $exe = Join-Path $installDir $entry.MainBinaryName
    return [pscustomobject]@{ Entry = $entry; Exe = $exe }
}
function Get-Links {
    if (-not (Test-Path -LiteralPath $startMenu)) { return @() }
    return @(Get-ChildItem -LiteralPath $startMenu -Recurse -Filter 'AKM.lnk' -ErrorAction SilentlyContinue)
}
function Get-LinkTarget([string]$path) { (New-Object -ComObject WScript.Shell).CreateShortcut($path).TargetPath }
function Assert-Installed {
    Assert-True (Test-Path -LiteralPath $uninstallKey) 'the uninstall registry key is missing'
    $installed = Get-Installed
    Assert-True ($installed.Entry.DisplayVersion -eq '0.1.0') "DisplayVersion is '$($installed.Entry.DisplayVersion)'"
    Assert-True (Test-Path -LiteralPath $installed.Exe -PathType Leaf) "$($installed.Exe) is missing"
    Assert-True (Test-Path -LiteralPath (Join-Path $installDir 'uninstall.exe') -PathType Leaf) 'uninstall.exe is missing'
    $links = Get-Links
    Assert-True ($links.Count -eq 1) "expected one Start menu AKM.lnk, found $($links.Count)"
    $target = Get-LinkTarget $links[0].FullName
    Assert-True ([string]::Equals($target, $installed.Exe, [StringComparison]::OrdinalIgnoreCase)) "the Start menu entry points at $target"
    $binding = Assert-InstalledBuild -Installed $installed.Exe -Built $desktopFull
    return "exe=$($installed.Exe) $binding startMenu=$($links[0].FullName) desktopLink=$(Test-Path -LiteralPath $desktopLink) webView2=$(Get-WebView2Version)"
}
# The setup program installs the WebView2 runtime when it is missing (embedBootstrapper), so the
# install step records the version that the app then uses.
function Get-WebView2Version {
    $guid = '{F3017226-FE2A-4295-8BDF-00C3A9A7E4C5}'
    foreach ($key in @("HKLM:\SOFTWARE\WOW6432Node\Microsoft\EdgeUpdate\Clients\$guid", "HKLM:\SOFTWARE\Microsoft\EdgeUpdate\Clients\$guid", "HKCU:\SOFTWARE\Microsoft\EdgeUpdate\Clients\$guid")) {
        $value = (Get-ItemProperty -LiteralPath $key -Name pv -ErrorAction SilentlyContinue).pv
        if ($value -and $value -ne '0.0.0.0') { return $value }
    }
    return 'none'
}

$userData = $null
try {
    Step 'setup program identity' {
        $signature = Get-AuthenticodeSignature -LiteralPath $setupFull
        "sha256=$((Get-FileHash -LiteralPath $setupFull -Algorithm SHA256).Hash.ToLowerInvariant()) authenticode=$($signature.Status)"
    }
    Step 'fake user data before the install' {
        Invoke-Cli @('init', '--name', 'Fake install-test vault') | Out-Null
        Invoke-Cli @('add', 'Fake install key', '--project', 'demo', '--env', 'dev', '--env-var', 'FAKE_INSTALL_KEY', '--secret-stdin') 'fake-install-secret-0123456789' | Out-Null
        $out = Invoke-Cli @('get', 'Fake install key', '--field', 'secret')
        Assert-True ($out.Trim() -eq 'fake-install-secret-0123456789') 'the fake key does not read back'
        Assert-True (Test-Path -LiteralPath $journalDir -PathType Container) "the CLI wrote no journal in $journalDir"
        New-Item -ItemType Directory -Path $settingsDir | Out-Null
        [IO.File]::WriteAllText($settings, (@{ vaultPath = $vault } | ConvertTo-Json -Compress), (New-Object Text.UTF8Encoding $false))
        $script:userData = Get-UserData
        "$($userData.Count) files: the vault, the settings and the journal"
    }
    Step 'silent install for the current user' {
        $run = Start-Process -FilePath $setupFull -ArgumentList '/S' -Wait -PassThru
        Assert-True ($run.ExitCode -eq 0) "the setup program exited $($run.ExitCode)"
        Assert-Installed
    }
    Step 'user data unchanged by the install' { Assert-SameUserData $userData }
    Step 'installed app starts and passes the focused proof (-JournalOnly)' {
        $installed = Get-Installed
        $log = Join-Path $Evidence 'proof-journal-only.txt'
        & powershell -NoProfile -ExecutionPolicy Bypass -File $ProofScript -DesktopExe $installed.Exe -CliExe $cliFull -JournalOnly *>&1 | Tee-Object -FilePath $log | Out-Null
        $code = $LASTEXITCODE
        Assert-True ($code -eq 0) "the proof script exited $code (see proof-journal-only.txt)"
        "$(@(Select-String -LiteralPath $log -Pattern '^PASS:').Count) PASS lines"
    }
    Step 'user data unchanged by the app run' { Assert-SameUserData $userData }
    Step 'second install over the first (same version)' {
        $run = Start-Process -FilePath $setupFull -ArgumentList '/S' -Wait -PassThru
        Assert-True ($run.ExitCode -eq 0) "the setup program exited $($run.ExitCode)"
        Assert-Installed
    }
    Step 'user data unchanged by the second install' { Assert-SameUserData $userData }
    Step 'silent uninstall' {
        $uninstaller = Join-Path $installDir 'uninstall.exe'
        $exe = (Get-Installed).Exe
        $run = Start-Process -FilePath $uninstaller -ArgumentList '/S' -Wait -PassThru
        Assert-True ($run.ExitCode -eq 0) "the uninstaller exited $($run.ExitCode)"
        # An NSIS uninstaller copies itself to the temp folder and returns; wait for its work.
        $deadline = [DateTime]::UtcNow.AddMinutes(3)
        while ([DateTime]::UtcNow -lt $deadline -and ((Test-Path -LiteralPath $exe) -or (Test-Path -LiteralPath $uninstallKey) -or (Test-Path -LiteralPath $uninstaller))) { Start-Sleep -Seconds 2 }
        Assert-True (-not (Test-Path -LiteralPath $exe)) "$exe remains"
        Assert-True (-not (Test-Path -LiteralPath $uninstaller)) 'uninstall.exe remains'
        Assert-True (-not (Test-Path -LiteralPath $uninstallKey)) 'the uninstall registry key remains'
        Assert-True ((Get-Links).Count -eq 0) 'a Start menu AKM.lnk remains'
        Assert-True (-not (Test-Path -LiteralPath $desktopLink)) 'the desktop AKM.lnk remains'
        $left = if (Test-Path -LiteralPath $installDir) { @(Get-ChildItem -LiteralPath $installDir -Force | ForEach-Object Name) } else { @() }
        Assert-True (@($left | Where-Object { $_ -ne 'journal' }).Count -eq 0) "the install folder still holds: $($left -join ', ')"
        "install folder after uninstall: $(if ($left.Count) { $left -join ', ' } else { 'gone' })"
    }
    Step 'vault, settings and journal remain after the uninstall' { Assert-SameUserData $userData }
} finally {
    $steps | ConvertTo-Json -Depth 4 | Set-Content -LiteralPath (Join-Path $Evidence 'steps.json') -Encoding UTF8
    Write-Output "RESULT: $(if ($failed) { 'FAIL' } else { 'PASS' }) ($($steps.Count) steps; evidence in $Evidence)"
}
