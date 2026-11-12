# The native-proof tools refuse to run on a computer that is not a disposable runner, and they refuse
# when AKM state exists. Each case runs the real tool as a child process whose APPDATA, LOCALAPPDATA,
# RUNNER_TEMP and TEMP point at a new fake profile folder with sentinel files, and checks that the tool
# printed REFUSED, exited non-zero, and left the fake profile exactly as it was (class F).
# The setup program and the CLI that the cases pass are text sentinels, so a tool that got past its
# guards by mistake would fail at its first process start, inside the fake profile.
# No case installs, uninstalls or touches the real profile.
# Usage: powershell -NoProfile -ExecutionPolicy Bypass -File tools/test_native_proof_guards.ps1
$ErrorActionPreference = 'Stop'
$installTool = Join-Path $PSScriptRoot 'test_install.ps1'
$preflightTool = Join-Path $PSScriptRoot 'ci_native_preflight.ps1'
$failures = 0

function Get-Tree([string]$root) {
    $state = [ordered]@{}
    foreach ($item in Get-ChildItem -LiteralPath $root -Recurse -Force) {
        $state[$item.FullName.Substring($root.Length)] = if ($item.PSIsContainer) { 'dir' } else { (Get-FileHash -LiteralPath $item.FullName -Algorithm SHA256).Hash }
    }
    return ($state.GetEnumerator() | ForEach-Object { "$($_.Key)=$($_.Value)" }) -join "`n"
}

function New-FakeProfile([string[]]$sentinels) {
    $root = Join-Path ([IO.Path]::GetTempPath()) ('akm-guard-test-' + [Guid]::NewGuid().ToString('N'))
    foreach ($folder in @('Roaming', 'Local', 'RunnerTemp', 'Temp', 'inputs')) { New-Item -ItemType Directory -Path (Join-Path $root $folder) | Out-Null }
    Set-Content -LiteralPath (Join-Path $root 'inputs\AKM_0.1.0_x64-setup.exe') -Value 'sentinel setup program, not an executable'
    Set-Content -LiteralPath (Join-Path $root 'inputs\akm.exe') -Value 'sentinel CLI, not an executable'
    Set-Content -LiteralPath (Join-Path $root 'inputs\SHA256SUMS.txt') -Value ('0' * 64 + '  akm.exe')
    foreach ($relative in $sentinels) {
        $path = Join-Path $root $relative
        New-Item -ItemType Directory -Force -Path (Split-Path -Parent $path) | Out-Null
        Set-Content -LiteralPath $path -Value "sentinel $relative"
    }
    return $root
}

function Invoke-Tool([string]$root, [string]$tool, [string[]]$arguments, [hashtable]$environment) {
    $psi = New-Object System.Diagnostics.ProcessStartInfo
    $psi.FileName = 'powershell.exe'
    $quoted = $arguments | ForEach-Object { if ($_ -match '\s') { '"' + $_ + '"' } else { $_ } }
    $psi.Arguments = "-NoProfile -ExecutionPolicy Bypass -File `"$tool`" $($quoted -join ' ')"
    $psi.UseShellExecute = $false
    $psi.RedirectStandardOutput = $true
    $psi.RedirectStandardError = $true
    foreach ($name in @('GITHUB_ACTIONS', 'RUNNER_ENVIRONMENT')) { $psi.EnvironmentVariables.Remove($name) }
    $psi.EnvironmentVariables['APPDATA'] = Join-Path $root 'Roaming'
    $psi.EnvironmentVariables['LOCALAPPDATA'] = Join-Path $root 'Local'
    $psi.EnvironmentVariables['RUNNER_TEMP'] = Join-Path $root 'RunnerTemp'
    $psi.EnvironmentVariables['TEMP'] = Join-Path $root 'Temp'
    $psi.EnvironmentVariables['TMP'] = Join-Path $root 'Temp'
    foreach ($name in $environment.Keys) { $psi.EnvironmentVariables[$name] = $environment[$name] }
    $process = [Diagnostics.Process]::Start($psi)
    $out = $process.StandardOutput.ReadToEnd() + $process.StandardError.ReadToEnd()
    $process.WaitForExit()
    return [pscustomobject]@{ Code = $process.ExitCode; Output = $out }
}

function Test-Refusal([string]$name, [string]$tool, [string[]]$arguments, [hashtable]$environment, [string[]]$sentinels, [string]$reason) {
    $root = New-FakeProfile $sentinels
    try {
        $before = Get-Tree $root
        $resolved = $arguments | ForEach-Object { $_.Replace('<root>', $root) }
        $run = Invoke-Tool $root $tool $resolved $environment
        $after = Get-Tree $root
        $problems = @()
        if ($run.Code -eq 0) { $problems += 'it exited 0' }
        if ($run.Output -notmatch 'REFUSED') { $problems += 'it printed no REFUSED line' }
        if ($run.Output -notmatch [regex]::Escape($reason)) { $problems += "it did not name the reason '$reason'" }
        if ($after -ne $before) { $problems += 'the fake profile changed' }
        if ($problems.Count) {
            Write-Output "FAIL $name ($($problems -join '; '))"
            Write-Output ('     output: ' + ($run.Output.Trim() -replace "`r?`n", ' | '))
            $script:failures++
        } else { Write-Output "ok   $name" }
    } finally {
        # The only removal in this test: the fake profile folder that New-FakeProfile made above.
        Remove-Item -LiteralPath $root -Recurse -Force
    }
}

$ci = @{ GITHUB_ACTIONS = 'true'; RUNNER_ENVIRONMENT = 'github-hosted' }
$install = @('-Setup', '<root>\inputs\AKM_0.1.0_x64-setup.exe', '-CliExe', '<root>\inputs\akm.exe', '-Evidence', '<root>\RunnerTemp\evidence')
$installSwitch = $install + '-DisposableRunner'

Test-Refusal 'install test refuses outside GitHub Actions' $installTool $installSwitch @{} @() "GITHUB_ACTIONS is '', not 'true'"
Test-Refusal 'install test refuses on a self-hosted runner' $installTool $installSwitch @{ GITHUB_ACTIONS = 'true'; RUNNER_ENVIRONMENT = 'self-hosted' } @() 'not disposable'
Test-Refusal 'install test refuses without -DisposableRunner' $installTool $install $ci @() 'the switch -DisposableRunner was not passed'
Test-Refusal 'install test refuses when the install or journal folder exists' $installTool $installSwitch $ci @('Local\AKM\journal\sentinel.txt') 'AKM state exists already'
Test-Refusal 'install test refuses when the settings folder exists' $installTool $installSwitch $ci @('Roaming\com.akm.desktop\settings.json') 'com.akm.desktop'
Test-Refusal 'install test refuses when the WebView2 data folder exists' $installTool $installSwitch $ci @('Local\com.akm.desktop\EBWebView\sentinel.txt') 'com.akm.desktop'
Test-Refusal 'install test refuses when a Start menu entry exists' $installTool $installSwitch $ci @('Roaming\Microsoft\Windows\Start Menu\Programs\AKM.lnk') 'AKM.lnk'

$preflight = @('-Artifacts', '<root>\inputs', '-Evidence', '<root>\RunnerTemp\evidence', '-InstallWebView2')
Test-Refusal 'preflight refuses outside GitHub Actions' $preflightTool ($preflight + '-DisposableRunner') @{} @() "GITHUB_ACTIONS is '', not 'true'"
Test-Refusal 'preflight refuses without -DisposableRunner' $preflightTool $preflight $ci @() 'the switch -DisposableRunner was not passed'

# Structural: neither tool removes anything itself. The install test's only removal is the AKM
# uninstaller, on the install that the test made.
foreach ($tool in @($installTool, $preflightTool, (Join-Path $PSScriptRoot 'DisposableRunner.psm1'))) {
    $tokens = $null; $errors = $null
    [void][Management.Automation.Language.Parser]::ParseFile($tool, [ref]$tokens, [ref]$errors)
    $removals = @($tokens | Where-Object { $_.Text -match '^(Remove-Item|rm|rmdir|del|erase|ri|Clear-Content|Delete|DeleteFile|RmDir)$' })
    if ($errors.Count -or $removals.Count) {
        Write-Output "FAIL $(Split-Path -Leaf $tool) has no removal ($($errors.Count) parse errors; removals: $(($removals | ForEach-Object Text) -join ', '))"
        $failures++
    } else { Write-Output "ok   $(Split-Path -Leaf $tool) has no removal" }
}

if ($failures) { Write-Output "$failures case(s) failed"; exit 1 }
Write-Output 'all cases passed'
