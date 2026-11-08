<#
.SYNOPSIS
  Runs the dependency checks: cargo audit, cargo deny and npm audit.

.DESCRIPTION
  1. cargo audit            known advisories in Cargo.lock (RustSec).
  2. cargo deny check       advisories, licenses, sources and bans (deny.toml).
  3. npm audit --omit=dev   known advisories in the production npm packages of desktop.

  The script runs every check, prints each result, and exits 1 when any check
  fails or cannot run. A check that cannot run, or that reports an unfinished
  lookup, is a failure, not a pass.

  Warnings do not fail cargo audit: an unmaintained or unsound crate is printed
  and must be read by a person (docs/FINDINGS.md records the ones found). A
  vulnerability, a denied license, a banned crate or an unknown source fails.

  cargo-audit and cargo-deny are taken from .scratch/cargo-tools/bin first
  (put at the front of PATH for this script only), then from PATH. Install them into the local folder with:
    cargo install --locked --root .scratch/cargo-tools cargo-deny cargo-audit

  The advisory databases come from GitHub (cargo) and the npm registry (npm).
  These checks need network access. AKM itself does not (see tools/check_no_network.py).
  -Offline uses the cached advisory databases only (cargo audit --no-fetch,
  cargo deny --offline). It needs databases from an earlier run.

.PARAMETER Offline
  Do not fetch advisory databases. Needs databases from an earlier run.
#>
[CmdletBinding()]
param([switch]$Offline)

$ErrorActionPreference = 'Continue'
$root = Split-Path -Parent $PSScriptRoot
$localTools = Join-Path $root '.scratch/cargo-tools/bin'
$results = [ordered]@{}

function Test-CargoSubcommand([string]$name) {
  $local = Join-Path $localTools "cargo-$name.exe"
  if (Test-Path -LiteralPath $local) { return $true }
  return [bool](Get-Command "cargo-$name" -ErrorAction SilentlyContinue)
}

if (Test-Path -LiteralPath $localTools) { $env:PATH = "$localTools;$env:PATH" }

# A tool that prints this text could not finish a part of its work, even when
# it exits 0 (cargo audit does so for a failed yanked-crate lookup).
$incompleteText = "couldn't check|could not be completed|failed to fetch|failed to load"

function Invoke-Check([string]$name, [string]$directory, [scriptblock]$body) {
  Write-Host "=== $name"
  Push-Location -LiteralPath $directory
  try {
    $lines = @(& $body 2>&1 | ForEach-Object { "$_" })
    $code = $LASTEXITCODE
    $lines | Write-Host
    if ($code -eq 0 -and ($lines -match $incompleteText)) {
      Write-Host "ERROR: $name exited 0 but reported that part of the check did not finish."
      $code = 3
    }
  } catch {
    Write-Host "ERROR: $($_.Exception.Message)"
    $code = 2
  } finally {
    Pop-Location
  }
  $results[$name] = $code
  Write-Host "--- $name exit code $code"
}

if (Test-CargoSubcommand 'audit') {
  $auditArgs = @('audit')
  if ($Offline) { $auditArgs += '--no-fetch' }
  Invoke-Check 'cargo audit' $root { cargo @auditArgs }
} else {
  Write-Host 'ERROR: cargo-audit not found. Install it into .scratch/cargo-tools (see the script header).'
  $results['cargo audit'] = 127
}

if (Test-CargoSubcommand 'deny') {
  $denyArgs = @('deny')
  if ($Offline) { $denyArgs += '--offline' }
  $denyArgs += @('check', 'advisories', 'licenses', 'sources', 'bans')
  Invoke-Check 'cargo deny' $root { cargo @denyArgs }
} else {
  Write-Host 'ERROR: cargo-deny not found. Install it into .scratch/cargo-tools (see the script header).'
  $results['cargo deny'] = 127
}

Invoke-Check 'npm audit' (Join-Path $root 'desktop') { npm audit --omit=dev }

Write-Host ''
Write-Host '=== Summary'
$failed = $false
foreach ($name in $results.Keys) {
  $code = $results[$name]
  $label = if ($code -eq 0) { 'PASS' } else { 'FAIL' }
  if ($code -ne 0) { $failed = $true }
  Write-Host ("{0,-4} {1} (exit {2})" -f $label, $name, $code)
}
if ($failed) { exit 1 }
exit 0
