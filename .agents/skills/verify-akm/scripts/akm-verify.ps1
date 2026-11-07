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
#>
param(
  [Parameter(Mandatory = $true, Position = 0)][ValidateSet('new', 'cli', 'clean')][string]$Action,
  [string]$Feature,
  [string]$Akm,
  [string]$Session,
  [string]$Stdin,
  [string]$EvidenceRoot = 'W:\AKM-archive\proofs',
  [Parameter(ValueFromRemainingArguments = $true)][string[]]$CliArgs
)
$ErrorActionPreference = 'Stop'
$FakePassword = 'fake-verify-password-0000'

function Read-Session([string]$dir) {
  $file = Join-Path $dir 'session.json'
  if (!(Test-Path -LiteralPath $file)) { throw "Not an akm-verify session: $dir" }
  return Get-Content -LiteralPath $file -Raw | ConvertFrom-Json
}

switch ($Action) {
  'new' {
    if (!$Feature) { throw 'new needs -Feature <feature id from features/README.md>' }
    if (!$Akm -or !(Test-Path -LiteralPath $Akm)) { throw 'new needs -Akm <path to a built akm.exe>' }
    $stamp = [DateTime]::UtcNow.ToString('yyyyMMddTHHmmssZ')
    $dir = Join-Path ([IO.Path]::GetTempPath()) ("akm-verify-$Feature-" + [Guid]::NewGuid().ToString('N').Substring(0, 8))
    New-Item -ItemType Directory -Path $dir | Out-Null
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
    $s = Read-Session $Session
    $temp = [IO.Path]::GetFullPath([IO.Path]::GetTempPath())
    $full = [IO.Path]::GetFullPath($Session)
    if (!$full.StartsWith($temp, [StringComparison]::OrdinalIgnoreCase) -or (Split-Path -Leaf $full) -notlike 'akm-verify-*') {
      throw "Refusing to remove a folder that akm-verify did not create: $full"
    }
    Remove-Item -LiteralPath $full -Recurse -Force
    Write-Output "Removed $full. Evidence kept at $($s.evidence)"
  }
}
