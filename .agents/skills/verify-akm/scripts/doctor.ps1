<#
.SYNOPSIS
  Read-only check before an AKM verification run. Prints one OK, WARN or STOP line per check
  and exits 1 when any STOP line was printed.

.EXAMPLE
  ./.agents/skills/verify-akm/scripts/doctor.ps1 -Akm target\release\akm.exe -Desktop target\release\akm-desktop.exe
#>
param([string]$Akm, [string]$Desktop, [switch]$NeedClipboard)
$stop = $false
function Report([string]$level, [string]$text) {
  Write-Output "$level $text"
  if ($level -eq 'STOP') { $script:stop = $true }
}

$repo = (Resolve-Path (Join-Path $PSScriptRoot '..\..\..\..')).Path
Report 'OK' "repository $repo at $(git -C $repo rev-parse --short HEAD) on $(git -C $repo branch --show-current)"
$dirty = @(git -C $repo status --porcelain -- crates desktop tools)
if ($dirty.Count) { Report 'WARN' "source differs from HEAD ($($dirty.Count) paths): a build of this tree is not a build of HEAD" }

foreach ($pair in @(@('akm.exe', $Akm), @('akm-desktop.exe', $Desktop))) {
  if (!$pair[1]) { continue }
  if (Test-Path -LiteralPath $pair[1]) {
    $hash = (Get-FileHash -LiteralPath $pair[1] -Algorithm SHA256).Hash.ToLowerInvariant()
    Report 'OK' "$($pair[0]) sha256=$hash (compare with docs/STATE.md before a recorded proof)"
  } else { Report 'STOP' "$($pair[0]) not found at $($pair[1])" }
}

$running = @(Get-Process -Name 'akm-desktop' -ErrorAction SilentlyContinue)
if ($running.Count) { Report 'STOP' "an akm-desktop process is running (PID $($running.Id -join ', ')); never drive an instance this run did not start" }
else { Report 'OK' 'no akm-desktop process is running' }

$settings = Join-Path $env:APPDATA 'com.akm.desktop\settings.json'
if (Test-Path -LiteralPath $settings) {
  Report 'OK' "app settings sha256=$((Get-FileHash -LiteralPath $settings -Algorithm SHA256).Hash.ToLowerInvariant()); it must be byte-identical after the run"
} else { Report 'OK' 'no app settings file' }

$profileJournal = Join-Path $env:LOCALAPPDATA 'akm'
if (Test-Path -LiteralPath $profileJournal) { Report 'WARN' "$profileJournal exists: a run must never add to it; set AKM_JOURNAL_ROOT" }
else { Report 'OK' "no $profileJournal folder" }

if ($env:AKM_PASSWORD) { Report 'WARN' 'AKM_PASSWORD is set in this shell; a CLI run inherits it' }

$drive = Get-PSDrive -Name ($repo.Substring(0, 1))
if ($drive.Free -lt 10GB) { Report 'WARN' "drive $($drive.Name): has $([math]::Round($drive.Free / 1GB, 1)) GB free; set CARGO_TARGET_DIR to another drive" }

if ($NeedClipboard) {
  Add-Type -Namespace AkmDoctor -Name Clip -MemberDefinition '[DllImport("user32.dll")] public static extern int CountClipboardFormats();'
  if ([AkmDoctor.Clip]::CountClipboardFormats() -ne 0) { Report 'STOP' 'the clipboard has content; a clipboard flow needs an empty clipboard and the owner present' }
  else { Report 'OK' 'the clipboard is empty' }
}

if ($stop) { exit 1 }
