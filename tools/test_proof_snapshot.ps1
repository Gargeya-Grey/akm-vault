# Self-test for tools/ProofSnapshot.psm1. It needs no app, window, clipboard or vault.
# Run: powershell -NoProfile -ExecutionPolicy Bypass -File tools/test_proof_snapshot.ps1
# Each case is a scenario from the R4-04 proof script review of 1 Nov 2026.

$ErrorActionPreference = 'Stop'
Import-Module (Join-Path $PSScriptRoot 'ProofSnapshot.psm1') -Force
$failures = 0

function Case([string]$name, [scriptblock]$body) {
  Reset-ProofDismissed
  try { & $body; Write-Output "ok   $name" }
  catch { $script:failures += 1; Write-Output "FAIL $name :: $($_.Exception.Message)" }
}

function Expect([bool]$condition, [string]$what) { if (!$condition) { throw $what } }

function Expect-Throw([scriptblock]$body, [string]$pattern) {
  try { & $body } catch {
    if ($_.Exception.Message -match $pattern) { return }
    throw "threw the wrong error: $($_.Exception.Message)"
  }
  throw "did not throw (expected: $pattern)"
}

function N([string]$name, [string]$role = '', [string]$type = 'text', [object[]]$children = @()) {
  [pscustomobject]@{ Name = $name; Role = $role; Type = $type; Enabled = $true; Children = $children }
}

function Window([object[]]$children) { N 'AKM' '' 'window' (@(N 'Keys' '' 'button') + $children) }

function Outcome([string[]]$texts, [switch]$Wrapped) {
  $notices = @($texts | ForEach-Object { N '' 'status' 'text' @(N $_) })
  if ($Wrapped) { $notices = @(N '' '' 'group' $notices) }
  N 'Operation outcomes' 'region' 'group' ($notices + @(N 'Dismiss operation outcomes' '' 'button'))
}

# A reader that returns the given snapshots in order, then repeats the last one.
function New-SampleReader([object[]]$snapshots) {
  $state = @{ Index = 0 }
  { $i = [Math]::Min($state.Index, $snapshots.Count - 1); $state.Index += 1; $snapshots[$i] }.GetNewClosure()
}

$noRegion = { param($s) !$s.Region }
$cancelled = 'File export was cancelled'

Case 'text never matches across separate elements (finding 7)' {
  $s = ConvertTo-ProofSnapshot (Window @((N 'Import '), (N 'completed.'))) 'Keys'
  Expect (!(Test-ProofText $s 'Import completed\.')) 'split names matched'
  $s = ConvertTo-ProofSnapshot (Window @(N 'Import completed.')) 'Keys'
  Expect (Test-ProofText $s 'Import completed\.') 'a whole name did not match'
}

Case 'records are counted per notice, also inside a wrapper (finding 6)' {
  foreach ($wrapped in @($false, $true)) {
    $s = ConvertTo-ProofSnapshot (Window @(Outcome @('Import completed.', 'File export completed.') -Wrapped:$wrapped)) 'Keys'
    Expect ($s.Records.Count -eq 2) "found $($s.Records.Count) records (wrapped=$wrapped)"
  }
  $s = ConvertTo-ProofSnapshot (Window @(Outcome @('Import completed.'))) 'Keys'
  Expect ($s.Records.Count -eq 1 -and $s.Records[0].Text -eq 'Import completed.') 'one record was not read'
  $s = ConvertTo-ProofSnapshot (Window @()) 'Keys'
  Expect ($s.Records.Count -eq 0 -and !$s.Region) 'zero records were not read'
}

Case 'a reader error propagates and is never read as absence (finding 3)' {
  Expect-Throw { Wait-ProofSnapshot -Reader { throw 'fake provider failure' } -Predicate $noRegion -Milliseconds 300 } 'fake provider failure'
  Expect-Throw { Assert-ProofStable -Reader { throw 'fake provider failure' } -Predicate $noRegion -Milliseconds 300 -Description 'x' } 'fake provider failure'
}

Case 'a tree without its sentinel never counts as absence (finding 3)' {
  $unreadable = ConvertTo-ProofSnapshot (N 'AKM' '' 'window') 'Keys'
  Expect (!(Wait-ProofSnapshot -Reader (New-SampleReader @($unreadable)) -Predicate $noRegion -Milliseconds 300)) 'an unreadable tree counted as absence'
  Expect-Throw { Assert-ProofStable -Reader (New-SampleReader @($unreadable)) -Predicate $noRegion -Milliseconds 300 -Description 'x' } 'not readable'
}

Case 'a delayed but correct dismissal passes (finding 2)' {
  $shown = ConvertTo-ProofSnapshot (Window @(Outcome @('File export was cancelled. No file was saved by this request.'))) 'Keys'
  $gone = ConvertTo-ProofSnapshot (Window @()) 'Keys'
  $reader = New-SampleReader @($shown, $shown, $shown, $gone)
  Expect (Wait-ProofSnapshot -Reader $reader -Predicate $noRegion -Milliseconds 3000 -IntervalMs 10) 'the delayed dismissal was not seen'
  Add-ProofDismissed $cancelled
  Assert-ProofStable -Reader $reader -Predicate $noRegion -Milliseconds 200 -Description 'stays dismissed' -IntervalMs 10 | Out-Null
}

Case 'a recurrence after absence fails the stability check (finding 2)' {
  $shown = ConvertTo-ProofSnapshot (Window @(Outcome @('Import completed.'))) 'Keys'
  $gone = ConvertTo-ProofSnapshot (Window @()) 'Keys'
  Expect-Throw { Assert-ProofStable -Reader (New-SampleReader @($gone, $gone, $shown)) -Predicate $noRegion -Milliseconds 2000 -Description 'stays absent' -IntervalMs 10 } 'failed at sample 3'
}

Case 'a dismissed outcome that returns during a later step fails, before a later dismissal can hide it (finding 1)' {
  Add-ProofDismissed $cancelled
  $later = ConvertTo-ProofSnapshot (Window @(Outcome @('File export reported an error. This does not confirm that nothing was saved.', 'File export was cancelled. No file was saved by this request.'))) 'Keys'
  # The later step waits for its own error outcome; the returned cancellation must fail that wait.
  Expect-Throw { Wait-ProofSnapshot -Reader (New-SampleReader @($later)) -Predicate { param($s) @(Get-ProofRecords $s 'reported an error').Count -eq 1 } -Milliseconds 300 } 'dismissed outcome returned'
}

Case 'a delayed recurrence is caught by every later sample (finding 1)' {
  Add-ProofDismissed $cancelled
  $gone = ConvertTo-ProofSnapshot (Window @()) 'Keys'
  $back = ConvertTo-ProofSnapshot (Window @(Outcome @('File export was cancelled. No file was saved by this request.'))) 'Keys'
  $reader = New-SampleReader @($gone, $gone, $gone, $gone, $back)
  Expect-Throw { Wait-ProofSnapshot -Reader $reader -Predicate { param($s) $false } -Milliseconds 2000 -IntervalMs 10 } 'dismissed outcome returned'
}

Case 'the dialog subtree is read separately' {
  $tree = Window @(N 'OpenAI prod proof' 'dialog' 'window' @((N 'Reveal' '' 'button'), (N 'Copy' '' 'button')))
  $s = ConvertTo-ProofSnapshot $tree 'Copy'
  Expect ($s.Sentinel -and @($s.Dialog | Where-Object { $_.Name -eq 'Reveal' }).Count -eq 1) 'the dialog nodes were not read'
}

if ($failures -gt 0) { Write-Output "$failures case(s) failed"; exit 1 }
Write-Output 'all cases passed'
