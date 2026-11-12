# Self-test for .agents/skills/verify-akm/scripts/akm-verify.ps1 (finding FR-27, fault class F).
# `clean` must remove only a folder that `new` created in this run's state. It needs no app and
# no real vault. Every case runs the script in a child PowerShell with its own TEMP folder, state
# folder and evidence folder, so nothing outside .scratch-style temporary folders is touched.
# Run: powershell -NoProfile -ExecutionPolicy Bypass -File tools/test_akm_verify.ps1

$ErrorActionPreference = 'Stop'
$script = (Resolve-Path (Join-Path $PSScriptRoot '..\.agents\skills\verify-akm\scripts\akm-verify.ps1')).Path
$work = Join-Path ([IO.Path]::GetTempPath()) ("akm-verify-test-" + [Guid]::NewGuid().ToString('N').Substring(0, 8))
$fakeTemp = Join-Path $work 'temp'
$state = Join-Path $work 'state'
$evidence = Join-Path $work 'evidence'
New-Item -ItemType Directory -Path $fakeTemp, $state, $evidence | Out-Null
$fakeAkm = Join-Path $work 'fake-akm.exe'
Set-Content -LiteralPath $fakeAkm -Value 'not a real program' -Encoding ASCII
$failures = 0

# Runs akm-verify.ps1 in a child process whose TEMP is $fakeTemp. Returns the exit code and output.
function Invoke-AkmVerify([string[]]$Arguments) {
  $saved = @{ TEMP = $env:TEMP; TMP = $env:TMP; STATE = $env:AKM_VERIFY_STATE_ROOT }
  $env:TEMP = $fakeTemp; $env:TMP = $fakeTemp; $env:AKM_VERIFY_STATE_ROOT = $state
  $ErrorActionPreference = 'Continue' # a native error line is data here
  try {
    $lines = @(& powershell -NoProfile -ExecutionPolicy Bypass -File $script @Arguments 2>&1 | ForEach-Object { "$_" })
    # The console wraps a long error line. Compare without white space.
    return [pscustomobject]@{ ExitCode = $LASTEXITCODE; Text = ($lines -join "`n"); Flat = (($lines -join '') -replace '\s', '') }
  } finally {
    $env:TEMP = $saved.TEMP; $env:TMP = $saved.TMP; $env:AKM_VERIFY_STATE_ROOT = $saved.STATE
  }
}

function Case([string]$name, [scriptblock]$body) {
  try { & $body; Write-Output "ok   $name" }
  catch { $script:failures += 1; Write-Output "FAIL $name :: $($_.Exception.Message)" }
}

function Expect([bool]$condition, [string]$what) { if (!$condition) { throw $what } }

# A folder that only looks like a session: the right prefix, a readable session.json, a file that
# a person would miss. It is not made by `new`.
function New-Sentinel([string]$leaf) {
  $dir = Join-Path $fakeTemp $leaf
  New-Item -ItemType Directory -Path $dir | Out-Null
  $json = @{ feature = 'sentinel'; akm = $fakeAkm; vault = (Join-Path $dir 'vault.kdbx'); journalRoot = (Join-Path $dir 'journal'); evidence = $evidence } | ConvertTo-Json
  Set-Content -LiteralPath (Join-Path $dir 'session.json') -Value $json -Encoding UTF8
  Set-Content -LiteralPath (Join-Path $dir 'precious.txt') -Value 'keep me' -Encoding ASCII
  return $dir
}

function New-Session([string]$feature) {
  $run = Invoke-AkmVerify @('new', '-Feature', $feature, '-Akm', $fakeAkm, '-EvidenceRoot', $evidence)
  Expect ($run.ExitCode -eq 0) "new failed: $($run.Text)"
  $dir = ($run.Text -split "`n" | Where-Object { $_ -like "*akm-verify-$feature-*" } | Select-Object -Last 1).Trim()
  Expect (Test-Path -LiteralPath $dir) "new did not print an existing folder: $($run.Text)"
  return $dir
}

try {
  Case 'a folder with the right prefix and a readable session.json is not removed' {
    $dir = New-Sentinel 'akm-verify-sentinel-aaaa1111'
    $run = Invoke-AkmVerify @('clean', '-Session', $dir)
    Expect (Test-Path -LiteralPath (Join-Path $dir 'precious.txt')) 'the sentinel file was removed'
    Expect ($run.ExitCode -ne 0) 'clean exited 0 for a folder it did not create'
    Expect ($run.Flat.Contains(($dir -replace '\s', ''))) "the refusal does not name the folder: $($run.Text)"
  }

  Case 'a copied marker with the wrong token is refused' {
    $dir = New-Sentinel 'akm-verify-sentinel-bbbb2222'
    Set-Content -LiteralPath (Join-Path $dir '.akm-verify-owner') -Value ('0' * 64) -Encoding ASCII
    $run = Invoke-AkmVerify @('clean', '-Session', $dir)
    Expect (Test-Path -LiteralPath (Join-Path $dir 'precious.txt')) 'a forged marker removed the folder'
    Expect ($run.ExitCode -ne 0 -and $run.Flat.Contains(($dir -replace '\s', ''))) "no named refusal: $($run.Text)"
  }

  Case 'a marker that matches a state record of another folder is refused' {
    $mine = New-Session 'owned'
    $dir = New-Sentinel 'akm-verify-sentinel-cccc3333'
    Copy-Item -LiteralPath (Join-Path $mine '.akm-verify-owner') -Destination (Join-Path $dir '.akm-verify-owner')
    $run = Invoke-AkmVerify @('clean', '-Session', $dir)
    Expect (Test-Path -LiteralPath (Join-Path $dir 'precious.txt')) 'a copied marker removed the folder'
    Expect (Test-Path -LiteralPath $mine) 'the real session was removed by the wrong request'
    Expect ($run.ExitCode -ne 0) 'clean exited 0'
    $null = Invoke-AkmVerify @('clean', '-Session', $mine)
  }

  Case 'a session that new created is removed, and its evidence stays' {
    $dir = New-Session 'real'
    Expect (Test-Path -LiteralPath (Join-Path $dir '.akm-verify-owner')) 'new wrote no ownership marker'
    $run = Invoke-AkmVerify @('clean', '-Session', $dir)
    Expect ($run.ExitCode -eq 0) "clean failed for its own folder: $($run.Text)"
    Expect (!(Test-Path -LiteralPath $dir)) 'the session folder was not removed'
    Expect (@(Get-ChildItem -LiteralPath $evidence -Directory -Filter '*-real').Count -eq 1) 'the evidence folder was removed'
    Expect (@(Get-ChildItem -LiteralPath $state -File).Count -eq 0) 'the state record stayed after the removal'
  }

  Case 'a sentinel folder beside a real session survives a clean of the real one' {
    $sentinel = New-Sentinel 'akm-verify-sentinel-dddd4444'
    $dir = New-Session 'beside'
    $null = Invoke-AkmVerify @('clean', '-Session', $dir)
    Expect (!(Test-Path -LiteralPath $dir)) 'the real session stayed'
    Expect (Test-Path -LiteralPath (Join-Path $sentinel 'precious.txt')) 'the sentinel was removed'
  }
} finally {
  if ($work -and (Split-Path -Leaf $work) -like 'akm-verify-test-*' -and (Test-Path -LiteralPath $work)) {
    Remove-Item -LiteralPath $work -Recurse -Force
  }
}

if ($failures -gt 0) { Write-Output "$failures case(s) failed"; exit 1 }
Write-Output 'all cases passed'
