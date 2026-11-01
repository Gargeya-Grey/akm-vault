# Snapshot logic for the R4-04 outcome and Key Detail checks in verify_windows_app.ps1.
#
# A snapshot is one complete read of the owned AKM window:
#   Sentinel  [bool]      the named sentinel button was in the tree, so the tree was readable
#   Nodes     [object[]]  every element: Name, Role (ARIA role), Type, Enabled
#   Region    [bool]      the "Operation outcomes" region exists
#   Records   [object[]]  one per outcome notice in that region: Text (its own subtree, joined with spaces)
#   Dialog    [object[]]  the nodes inside the open role="dialog" element, if any
#
# Text matches run per element or per outcome record, never across unrelated elements.
# A reader error always propagates. It is never read as absence.
# Every sample also checks that no dismissed outcome has returned.

$script:Dismissed = [System.Collections.Generic.List[string]]::new()

function Reset-ProofDismissed { $script:Dismissed.Clear() }
function Add-ProofDismissed([string]$pattern) { $script:Dismissed.Add($pattern) }
function Get-ProofDismissed { $script:Dismissed.ToArray() }

function Get-ProofSubtree($node) {
  $list = [System.Collections.Generic.List[object]]::new()
  $stack = [System.Collections.Generic.Stack[object]]::new()
  $stack.Push($node)
  while ($stack.Count -gt 0) {
    $current = $stack.Pop()
    $list.Add($current)
    $children = @($current.Children)
    for ($i = $children.Count - 1; $i -ge 0; $i--) { $stack.Push($children[$i]) }
  }
  $list.ToArray()
}

# Converts one tree read (nodes with Name, Role, Type, Enabled, Children) into a snapshot.
# Outcome records are the role="status" and role="alert" notices inside the region, at any depth.
function ConvertTo-ProofSnapshot($tree, [string]$Sentinel) {
  $nodes = @(Get-ProofSubtree $tree)
  $region = @($nodes | Where-Object { $_.Name -ceq 'Operation outcomes' } | Select-Object -First 1)
  $records = @()
  if ($region.Count -eq 1) {
    $records = @(@(Get-ProofSubtree $region[0]) | Where-Object { $_.Role -in @('status', 'alert') } | ForEach-Object {
      [pscustomobject]@{ Text = ((@(Get-ProofSubtree $_) | ForEach-Object { "$($_.Name)" } | Where-Object { $_ }) -join ' ') }
    })
  }
  $dialog = @($nodes | Where-Object { $_.Role -eq 'dialog' } | Select-Object -First 1)
  [pscustomobject]@{
    Sentinel = [bool](@($nodes | Where-Object { $_.Name -ceq $Sentinel -and $_.Type -eq 'button' }).Count)
    Nodes    = $nodes
    Region   = ($region.Count -eq 1)
    Records  = $records
    Dialog   = $(if ($dialog.Count -eq 1) { @(Get-ProofSubtree $dialog[0]) } else { @() })
  }
}

function Test-ProofText($snapshot, [string]$pattern) {
  foreach ($node in $snapshot.Nodes) { if ("$($node.Name)" -match $pattern) { return $true } }
  foreach ($record in $snapshot.Records) { if ("$($record.Text)" -match $pattern) { return $true } }
  return $false
}

function Get-ProofRecords($snapshot, [string]$pattern) {
  @($snapshot.Records | Where-Object { "$($_.Text)" -match $pattern })
}

function Assert-ProofNoDismissedReturn($snapshot) {
  foreach ($pattern in $script:Dismissed) {
    if (@(Get-ProofRecords $snapshot $pattern).Count -gt 0 -or (Test-ProofText $snapshot $pattern)) {
      throw "FAIL: a dismissed outcome returned ($pattern)"
    }
  }
}

function Read-ProofSample([scriptblock]$Reader) {
  $snapshot = & $Reader
  if ($null -eq $snapshot) { throw 'The snapshot reader returned nothing.' }
  Assert-ProofNoDismissedReturn $snapshot
  return $snapshot
}

# Waits until one readable sample satisfies the predicate. Returns $true or $false.
# A sample without its sentinel never counts as a match, so absence needs a readable tree.
function Wait-ProofSnapshot([scriptblock]$Reader, [scriptblock]$Predicate, [int]$Milliseconds, [int]$IntervalMs = 100) {
  $until = [DateTime]::UtcNow.AddMilliseconds($Milliseconds)
  do {
    $snapshot = Read-ProofSample $Reader
    if ($snapshot.Sentinel -and (& $Predicate $snapshot)) { return $true }
    Start-Sleep -Milliseconds $IntervalMs
  } while ([DateTime]::UtcNow -lt $until)
  return $false
}

# Every sample for the whole period must be readable and satisfy the predicate. Throws on the first failure.
function Assert-ProofStable([scriptblock]$Reader, [scriptblock]$Predicate, [int]$Milliseconds, [string]$Description, [int]$IntervalMs = 100) {
  $until = [DateTime]::UtcNow.AddMilliseconds($Milliseconds)
  $samples = 0
  do {
    $snapshot = Read-ProofSample $Reader
    $samples += 1
    if (!$snapshot.Sentinel) { throw "FAIL: $Description (the owned tree was not readable at sample $samples)" }
    if (!(& $Predicate $snapshot)) { throw "FAIL: $Description (failed at sample $samples)" }
    Start-Sleep -Milliseconds $IntervalMs
  } while ([DateTime]::UtcNow -lt $until)
  if ($samples -lt 2) { throw "FAIL: $Description (only $samples sample)" }
  "PASS: $Description ($samples samples)"
}

Export-ModuleMember -Function Get-ProofSubtree, ConvertTo-ProofSnapshot, Reset-ProofDismissed, Add-ProofDismissed, Get-ProofDismissed, Test-ProofText, Get-ProofRecords, Assert-ProofNoDismissedReturn, Read-ProofSample, Wait-ProofSnapshot, Assert-ProofStable
