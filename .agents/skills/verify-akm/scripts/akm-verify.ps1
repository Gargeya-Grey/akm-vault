<#
.SYNOPSIS
  Drives the AKM CLI against a disposable fake vault and records evidence.

.EXAMPLE
  $s = ./.agents/skills/verify-akm/scripts/akm-verify.ps1 new -Feature vault-and-keys -Akm C:\build\release\akm.exe
  ./.agents/skills/verify-akm/scripts/akm-verify.ps1 cli -Session $s -- init --name "Fake vault"
  './.agents/skills/verify-akm/scripts/akm-verify.ps1 cli -Session $s -Stdin 'fake-secret-1234' -- add "Fake key" --project demo --env dev --secret-stdin'
  ./.agents/skills/verify-akm/scripts/akm-verify.ps1 clean -Session $s

  `new` prints the session folder. `cli` runs one CLI command with the session's fake vault,
  fake password and private test journal root, and appends the command, stdout, stderr and
  exit code to the evidence transcript. `clean` removes the fake vault folder and keeps the evidence.

  `new` writes a random token to a marker in the session folder and to a record of its own
  (AKM_VERIFY_STATE_ROOT, default %LOCALAPPDATA%\akm-verify\state). `cli` and `clean` act only on a
  folder whose marker matches that record. Any other folder stays, and the error names it.

  The evidence folder is -EvidenceRoot, else AKM_EVIDENCE_ROOT, else %LOCALAPPDATA%\akm-verify\proofs.
#>
param(
  [Parameter(Mandatory = $true, Position = 0)][ValidateSet('new', 'cli', 'clean')][string]$Action,
  [string]$Feature,
  [string]$Akm,
  [string]$Session,
  [string]$Stdin,
  [string]$EvidenceRoot = $(if ($env:AKM_EVIDENCE_ROOT) { $env:AKM_EVIDENCE_ROOT } else { Join-Path $env:LOCALAPPDATA 'akm-verify\proofs' }),
  [Parameter(ValueFromRemainingArguments = $true)][string[]]$CliArgs
)
$ErrorActionPreference = 'Stop'
$FakePassword = 'fake-verify-password-0000'
$MarkerName = '.akm-verify-owner'

# Own state: one file per session folder, outside that folder. `new` writes a random token to it
# and to the marker inside the folder. `clean` acts only when both hold the same token (FR-27).
$StateRoot = if ($env:AKM_VERIFY_STATE_ROOT) { $env:AKM_VERIFY_STATE_ROOT } else { Join-Path $env:LOCALAPPDATA 'akm-verify\state' }

function New-OwnerToken {
  $bytes = New-Object byte[] 32
  $rng = [Security.Cryptography.RandomNumberGenerator]::Create()
  try { $rng.GetBytes($bytes) } finally { $rng.Dispose() }
  return -join ($bytes | ForEach-Object { $_.ToString('x2') })
}

function Get-StateFile([string]$leaf) { Join-Path $StateRoot "$leaf.token" }

# Returns the full path when the folder is a session that this script created. Throws, naming the
# folder and the reason, when it is not. A folder that fails the check stays where it is.
function Assert-OwnedSession([string]$dir) {
  $full = [IO.Path]::GetFullPath($dir).TrimEnd('\')
  $leaf = Split-Path -Leaf $full
  $temp = [IO.Path]::GetFullPath([IO.Path]::GetTempPath()).TrimEnd('\')
  $why = $null
  if ($leaf -notlike 'akm-verify-*') { $why = 'its name does not start with akm-verify-' }
  elseif (![string]::Equals([IO.Path]::GetDirectoryName($full), $temp, [StringComparison]::OrdinalIgnoreCase)) { $why = 'it is not directly inside the temp folder' }
  elseif (!(Test-Path -LiteralPath $full -PathType Container)) { $why = 'it is not a folder' }
  elseif ((Get-Item -LiteralPath $full -Force).Attributes -band [IO.FileAttributes]::ReparsePoint) { $why = 'it is a junction or link' }
  else {
    $marker = Join-Path $full $MarkerName
    $stateFile = Get-StateFile $leaf
    if (!(Test-Path -LiteralPath $marker -PathType Leaf)) { $why = 'it has no ownership marker' }
    elseif (!(Test-Path -LiteralPath $stateFile -PathType Leaf)) { $why = 'this script has no record of creating it' }
    else {
      $inFolder = (Get-Content -LiteralPath $marker -Raw).Trim()
      $recorded = (Get-Content -LiteralPath $stateFile -Raw).Trim()
      if ($inFolder.Length -lt 64 -or $inFolder -cne $recorded) { $why = 'its ownership token does not match the record' }
    }
  }
  if ($why) { throw "Refusing to use $full, because $why. The folder was left in place; akm-verify did not create it." }
  return $full
}

function Read-Session([string]$dir) {
  $full = Assert-OwnedSession $dir
  $file = Join-Path $full 'session.json'
  if (!(Test-Path -LiteralPath $file)) { throw "Not an akm-verify session: $full" }
  return Get-Content -LiteralPath $file -Raw | ConvertFrom-Json
}

switch ($Action) {
  'new' {
    if (!$Feature) { throw 'new needs -Feature <feature id from features/README.md>' }
    if (!$Akm -or !(Test-Path -LiteralPath $Akm)) { throw 'new needs -Akm <path to a built akm.exe>' }
    $stamp = [DateTime]::UtcNow.ToString('yyyyMMddTHHmmssZ')
    $dir = Join-Path ([IO.Path]::GetTempPath()) ("akm-verify-$Feature-" + [Guid]::NewGuid().ToString('N').Substring(0, 8))
    New-Item -ItemType Directory -Path $dir | Out-Null
    $token = New-OwnerToken
    New-Item -ItemType Directory -Force -Path $StateRoot | Out-Null
    Set-Content -LiteralPath (Get-StateFile (Split-Path -Leaf $dir)) -Value $token -Encoding ASCII
    Set-Content -LiteralPath (Join-Path $dir $MarkerName) -Value $token -Encoding ASCII
    $evidence = Join-Path $EvidenceRoot "$stamp-$Feature"
    if (Test-Path -LiteralPath $evidence) { throw "Evidence folder already exists: $evidence" }
    New-Item -ItemType Directory -Path $evidence | Out-Null
    $akmFull = (Resolve-Path -LiteralPath $Akm).Path
    $hash = (Get-FileHash -LiteralPath $akmFull -Algorithm SHA256).Hash.ToLowerInvariant()
    $state = [ordered]@{
      feature = $Feature; akm = $akmFull; akmSha256 = $hash
      vault = (Join-Path $dir 'vault.kdbx'); journalRoot = (Join-Path $dir 'journal')
      evidence = $evidence; created = $stamp
    }
    $state | ConvertTo-Json | Set-Content -LiteralPath (Join-Path $dir 'session.json') -Encoding UTF8
    "feature=$Feature akm=$akmFull sha256=$hash" | Set-Content -LiteralPath (Join-Path $evidence 'transcript.txt') -Encoding UTF8
    Write-Output $dir
  }
  'cli' {
    $s = Read-Session $Session
    $psi = New-Object System.Diagnostics.ProcessStartInfo
    $psi.FileName = $s.akm
    $psi.Arguments = ($CliArgs | ForEach-Object { if ($_ -match '[\s"]') { '"' + ($_ -replace '"', '\"') + '"' } else { $_ } }) -join ' '
    $psi.UseShellExecute = $false
    $psi.RedirectStandardOutput = $true
    $psi.RedirectStandardError = $true
    $psi.RedirectStandardInput = $true
    $psi.EnvironmentVariables['AKM_VAULT'] = $s.vault
    $psi.EnvironmentVariables['AKM_PASSWORD'] = $FakePassword
    $psi.EnvironmentVariables['AKM_JOURNAL_ROOT'] = $s.journalRoot
    $p = [System.Diagnostics.Process]::Start($psi)
    if ($PSBoundParameters.ContainsKey('Stdin')) { $p.StandardInput.WriteLine($Stdin) }
    $p.StandardInput.Close()
    $out = $p.StandardOutput.ReadToEnd()
    $err = $p.StandardError.ReadToEnd()
    $p.WaitForExit()
    $record = @("", "> akm $($psi.Arguments)", "exit=$($p.ExitCode)", "--- stdout", $out.TrimEnd(), "--- stderr", $err.TrimEnd())
    Add-Content -LiteralPath (Join-Path $s.evidence 'transcript.txt') -Value $record -Encoding UTF8
    Write-Output ([pscustomobject]@{ ExitCode = $p.ExitCode; Stdout = $out; Stderr = $err })
  }
  'clean' {
    $full = Assert-OwnedSession $Session
    $s = Read-Session $full
    Remove-Item -LiteralPath $full -Recurse -Force
    Remove-Item -LiteralPath (Get-StateFile (Split-Path -Leaf $full)) -Force
    Write-Output "Removed $full. Evidence kept at $($s.evidence)"
  }
}
