param(
  [Parameter(Mandatory = $true)][string]$DesktopExe,
  [Parameter(Mandatory = $true)][string]$CliExe,
  [string]$KeePassXC,
  [switch]$SkipClipboard,
  [switch]$RemainingOnly,
  [switch]$JournalOnly,
  [switch]$JournalExportOnly,
  [switch]$OutcomeOnly,
  [switch]$OutcomeRehearsal,
  [switch]$OwnerReadyForVisibility
)

$ErrorActionPreference = 'Stop'
if ($OutcomeOnly -and !$OwnerReadyForVisibility) { throw 'Outcome visibility proof requires the owner to confirm readiness first; then pass -OwnerReadyForVisibility. No unattended minimize attempt is permitted.' }
if ($OutcomeOnly -and $JournalExportOnly) { throw 'Choose either -OutcomeOnly or -JournalExportOnly.' }
if ($OutcomeRehearsal -and ($OutcomeOnly -or $JournalExportOnly -or $OwnerReadyForVisibility)) { throw 'The rehearsal never minimizes: use -OutcomeRehearsal alone.' }
if ($JournalExportOnly -or $OutcomeOnly -or $OutcomeRehearsal) { $JournalOnly = $true }
if ($JournalOnly -and $RemainingOnly) { throw 'Choose either -JournalOnly or -RemainingOnly, not both.' }
if ($JournalOnly) { $SkipClipboard = $true }
Add-Type -AssemblyName UIAutomationClient, UIAutomationTypes
Import-Module (Join-Path $PSScriptRoot 'ProofSnapshot.psm1') -Force
Add-Type @'
using System;
using System.Runtime.InteropServices;
using System.Text;
public static class AkmProofClipboard {
  public delegate bool EnumWindowProc(IntPtr window, IntPtr parameter);
  [DllImport("user32.dll")] public static extern bool EnumWindows(EnumWindowProc callback, IntPtr parameter);
  [DllImport("user32.dll")] public static extern uint GetWindowThreadProcessId(IntPtr window, out uint process);
  [DllImport("user32.dll", CharSet=CharSet.Unicode)] public static extern int GetClassName(IntPtr window, StringBuilder name, int length);
  [DllImport("user32.dll")] public static extern bool PostMessage(IntPtr window, uint message, UIntPtr wparam, IntPtr lparam);
  [DllImport("user32.dll")] public static extern bool IsIconic(IntPtr window);
  [DllImport("user32.dll", EntryPoint="SendMessageTimeoutW", CharSet=CharSet.Unicode, SetLastError=true)]
  static extern IntPtr SendText(IntPtr window, uint message, UIntPtr wparam, string text, uint flags, uint timeout, out UIntPtr result);
  [DllImport("user32.dll", EntryPoint="SendMessageTimeoutW", CharSet=CharSet.Unicode, SetLastError=true)]
  static extern IntPtr ReadText(IntPtr window, uint message, UIntPtr wparam, StringBuilder text, uint flags, uint timeout, out UIntPtr result);
  [DllImport("user32.dll")] public static extern IntPtr GetDlgItem(IntPtr dialog, int id);
  [DllImport("user32.dll")] public static extern IntPtr GetAncestor(IntPtr window, uint flags);
  [DllImport("user32.dll")] public static extern IntPtr GetProcessWindowStation();
  [DllImport("user32.dll")] public static extern IntPtr GetThreadDesktop(uint thread);
  [DllImport("kernel32.dll")] public static extern uint GetCurrentThreadId();
  [DllImport("user32.dll", SetLastError=true)] public static extern IntPtr OpenInputDesktop(uint flags, bool inherit, uint access);
  [DllImport("user32.dll")] public static extern bool CloseDesktop(IntPtr desktop);
  [DllImport("user32.dll", EntryPoint="GetUserObjectInformationW", CharSet=CharSet.Unicode, SetLastError=true)]
  static extern bool GetObjectName(IntPtr handle, int index, StringBuilder name, uint length, out uint needed);
  [DllImport("user32.dll")] public static extern bool OpenClipboard(IntPtr owner);
  [DllImport("user32.dll")] public static extern bool CloseClipboard();
  [DllImport("user32.dll")] public static extern int CountClipboardFormats();
  [DllImport("user32.dll", CharSet=CharSet.Unicode)] public static extern uint RegisterClipboardFormat(string name);
  [DllImport("user32.dll")] public static extern bool IsClipboardFormatAvailable(uint format);
  [DllImport("user32.dll")] public static extern IntPtr GetClipboardData(uint format);
  [DllImport("kernel32.dll")] public static extern IntPtr GlobalLock(IntPtr handle);
  [DllImport("kernel32.dll")] public static extern bool GlobalUnlock(IntPtr handle);
  [DllImport("user32.dll")] public static extern uint GetClipboardSequenceNumber();
  [DllImport("user32.dll")] public static extern bool GetWindowDisplayAffinity(IntPtr window, out uint affinity);
  static string ObjectName(IntPtr handle) {
    var name = new StringBuilder(256);
    uint needed;
    return GetObjectName(handle, 2, name, (uint)(name.Capacity * 2), out needed)
      ? name.ToString() : "unavailable:" + Marshal.GetLastWin32Error();
  }
  public static string DesktopDescription() {
    var input = OpenInputDesktop(0, false, 1);
    var inputName = input == IntPtr.Zero ? "unavailable:" + Marshal.GetLastWin32Error() : ObjectName(input);
    if (input != IntPtr.Zero) CloseDesktop(input);
    return "station=" + ObjectName(GetProcessWindowStation())
      + "; threadDesktop=" + ObjectName(GetThreadDesktop(GetCurrentThreadId())) + "; inputDesktop=" + inputName;
  }
  public static bool SetVerifiedEditText(IntPtr edit, string text) {
    UIntPtr result;
    if (SendText(edit, 0x000C, UIntPtr.Zero, text, 2, 1000, out result) == IntPtr.Zero || result == UIntPtr.Zero) return false;
    var actual = new StringBuilder(text.Length + 1);
    if (ReadText(edit, 0x000D, (UIntPtr)(uint)actual.Capacity, actual, 2, 1000, out result) == IntPtr.Zero) return false;
    return actual.ToString() == text;
  }
  public static IntPtr MonitorWindow(uint processId) {
    IntPtr found = IntPtr.Zero;
    EnumWindows((window, parameter) => {
      uint owner;
      GetWindowThreadProcessId(window, out owner);
      if (owner == processId) {
        var name = new StringBuilder(256);
        GetClassName(window, name, name.Capacity);
        if (name.ToString().StartsWith("AKMSession-")) { found = window; return false; }
      }
      return true;
    }, IntPtr.Zero);
    return found;
  }
}
'@

$desktop = (Resolve-Path -LiteralPath $DesktopExe).Path
$cli = (Resolve-Path -LiteralPath $CliExe).Path
$work = Join-Path $env:TEMP ('opencode\akm-app-proof-' + [Guid]::NewGuid())
New-Item -ItemType Directory -Path $work | Out-Null
$vault = Join-Path $work 'vault.kdbx'
$mainVault = $vault
$cloneVault = Join-Path $work 'clones.kdbx'
$legacyVault = Join-Path $work 'older.kdbx'
$secondVault = Join-Path $work 'second.kdbx'
$password = 'fake desktop proof correct horse battery staple'
$mainPassword = $password
$ownedVaults = @($vault, $cloneVault, $legacyVault, $secondVault)
$settingsPath = Join-Path $env:APPDATA 'com.akm.desktop\settings.json'
$settingsBefore = if (Test-Path -LiteralPath $settingsPath) { [IO.File]::ReadAllBytes($settingsPath) } else { $null }
$recovery = [ordered]@{
  desktopExe = $desktop
  desktopSha256 = (Get-FileHash -Algorithm SHA256 -LiteralPath $desktop).Hash
  settingsPath = $settingsPath
  hadSettings = ($null -ne $settingsBefore)
  ownedVaults = $ownedVaults
  processId = $null
  processStartUtc = $null
  remainingOnly = [bool]$RemainingOnly
  journalOnly = [bool]$JournalOnly
  journalExportOnly = [bool]$JournalExportOnly
  outcomeOnly = [bool]$OutcomeOnly
  ownerReadyForVisibility = [bool]$OwnerReadyForVisibility
}
if ($null -ne $settingsBefore) { [IO.File]::WriteAllBytes((Join-Path $work 'settings-before.bin'), $settingsBefore) }
$recovery | ConvertTo-Json | Set-Content -LiteralPath (Join-Path $work 'recovery.json') -Encoding UTF8
Write-Output "Proof recovery evidence: $work"
Write-Output "Proof executable SHA-256: $($recovery.desktopSha256)"
$process = $null
$success = $false
$AE = [System.Windows.Automation.AutomationElement]
$TS = [System.Windows.Automation.TreeScope]
# One cached read of the whole owned tree (raw view), so a snapshot never reads a removed element live.
$script:treeRequest = New-Object System.Windows.Automation.CacheRequest
# Managed UI Automation has no ARIA role property. WebView2 reports the role of a status, alert or
# dialog element as its localized control type, so the snapshot reads that as the role.
foreach ($property in @($AE::NameProperty, $AE::ControlTypeProperty, $AE::LocalizedControlTypeProperty, $AE::IsEnabledProperty)) { $script:treeRequest.Add([System.Windows.Automation.AutomationProperty]$property) }
$script:treeRequest.TreeScope = $TS::Subtree
$script:treeRequest.TreeFilter = [System.Windows.Automation.Condition]::TrueCondition

function Assert-Proof([bool]$condition, [string]$description) {
  if (!$condition) { throw "FAIL: $description" }
  Write-Output "PASS: $description"
}

function Save-ProofDiagnostics([string]$stage) {
  $details = [System.Collections.Generic.List[string]]::new()
  $details.Add("stage=$stage; harnessSession=$([Diagnostics.Process]::GetCurrentProcess().SessionId); $([AkmProofClipboard]::DesktopDescription())")
  if ($process -and !$process.HasExited) {
    $process.Refresh()
    $details.Add("testProcess=$($process.Id); testSession=$($process.SessionId); title=$($process.MainWindowTitle); mainHandle=$($process.MainWindowHandle)")
    $condition = New-Object System.Windows.Automation.PropertyCondition($AE::ProcessIdProperty, $process.Id)
    foreach ($window in $AE::RootElement.FindAll($TS::Children, $condition)) {
      $details.Add("window: name=$($window.Current.Name); class=$($window.Current.ClassName); handle=$($window.Current.NativeWindowHandle)")
      foreach ($element in $window.FindAll($TS::Descendants, [System.Windows.Automation.Condition]::TrueCondition)) {
        $details.Add("  $($element.Current.ControlType.ProgrammaticName) [$($element.Current.LocalizedControlType)]: $($element.Current.Name); class=$($element.Current.ClassName); id=$($element.Current.AutomationId); handle=$($element.Current.NativeWindowHandle)")
      }
    }
  }
  $details | Set-Content -LiteralPath (Join-Path $work 'native-diagnostics.txt') -Encoding UTF8
  $details | Select-Object -First 12 | Write-Output
}

function Assert-EmptyClipboard {
  if (![AkmProofClipboard]::OpenClipboard([IntPtr]::Zero)) { throw 'The clipboard is busy. Use -SkipClipboard to preserve it.' }
  try {
    if ([AkmProofClipboard]::CountClipboardFormats() -ne 0) { throw 'Refusing to replace a non-empty clipboard. Use -SkipClipboard or clear it yourself first.' }
  } finally { [void][AkmProofClipboard]::CloseClipboard() }
}

function Find-Control([string]$name, $type = $null, [int]$milliseconds = 12000) {
  $condition = New-Object System.Windows.Automation.PropertyCondition($AE::NameProperty, $name)
  if ($type) {
    $condition = New-Object System.Windows.Automation.AndCondition($condition, (New-Object System.Windows.Automation.PropertyCondition($AE::ControlTypeProperty, $type)))
  }
  $until = [DateTime]::UtcNow.AddMilliseconds($milliseconds)
  do {
    $found = $script:root.FindAll($TS::Descendants, $condition)
    if ($found.Count -gt 0) { return $found[$found.Count - 1] }
    Start-Sleep -Milliseconds 100
  } while ([DateTime]::UtcNow -lt $until)
  throw "Missing AKM control: $name"
}

function Set-Field([string]$name, [string]$value) {
  $element = Find-Control $name ([System.Windows.Automation.ControlType]::Edit)
  $element.GetCurrentPattern([System.Windows.Automation.ValuePattern]::Pattern).SetValue($value)
}

function Click-Control([string]$name) {
  # After an outcome is dismissed, every later step first checks that it has not returned.
  if (@(Get-ProofDismissed).Count -gt 0) { Read-ProofSample { Read-OwnedSnapshot '' } | Out-Null }
  $element = Find-Control $name ([System.Windows.Automation.ControlType]::Button)
  $element.GetCurrentPattern([System.Windows.Automation.InvokePattern]::Pattern).Invoke()
}

function Wait-Text([string]$pattern, [int]$milliseconds = 15000) {
  $until = [DateTime]::UtcNow.AddMilliseconds($milliseconds)
  do {
    $all = $script:root.FindAll($TS::Descendants, [System.Windows.Automation.Condition]::TrueCondition)
    $names = foreach ($element in $all) { $element.Current.Name }
    if (($names -join '') -match $pattern) { return $true }
    Start-Sleep -Milliseconds 100
  } while ([DateTime]::UtcNow -lt $until)
  return $false
}

function Invoke-TestCli([string[]]$words) {
  $previous = [Environment]::GetEnvironmentVariable('AKM_PASSWORD', 'Process')
  try {
    [Environment]::SetEnvironmentVariable('AKM_PASSWORD', $password, 'Process')
    $result = & $cli --vault $vault @words
    if ($LASTEXITCODE -ne 0) { throw "Fake-vault CLI failed: $($words[0])" }
    return $result
  } finally { [Environment]::SetEnvironmentVariable('AKM_PASSWORD', $previous, 'Process') }
}

function Check-ClipboardExclusions {
  if (![AkmProofClipboard]::OpenClipboard([IntPtr]::Zero)) { throw 'Clipboard is busy; run the proof again when it is not in use.' }
  try {
    foreach ($name in @('CanIncludeInClipboardHistory', 'CanUploadToCloudClipboard')) {
      $format = [AkmProofClipboard]::RegisterClipboardFormat($name)
      Assert-Proof ([AkmProofClipboard]::IsClipboardFormatAvailable($format)) "$name format is present"
      $handle = [AkmProofClipboard]::GetClipboardData($format)
      $memory = [AkmProofClipboard]::GlobalLock($handle)
      if ($memory -eq [IntPtr]::Zero) { throw "Cannot read our test clipboard flag $name" }
      try { Assert-Proof ([Runtime.InteropServices.Marshal]::ReadInt32($memory) -eq 0) "$name is disabled" }
      finally { [void][AkmProofClipboard]::GlobalUnlock($handle) }
    }
    $monitor = [AkmProofClipboard]::RegisterClipboardFormat('ExcludeClipboardContentFromMonitorProcessing')
    Assert-Proof ([AkmProofClipboard]::IsClipboardFormatAvailable($monitor)) 'clipboard monitoring exclusion is present'
    return [AkmProofClipboard]::GetClipboardSequenceNumber()
  } finally { [void][AkmProofClipboard]::CloseClipboard() }
}

function Unlock-FakeVault {
  Set-Field 'Master password' $password
  Click-Control 'Unlock vault'
  Find-Control 'Add key' ([System.Windows.Automation.ControlType]::Button) | Out-Null
}

function Send-TestSignal([uint32]$message, [uint32]$wparam) {
  $monitor = [AkmProofClipboard]::MonitorWindow([uint32]$process.Id)
  Assert-Proof ($monitor -ne [IntPtr]::Zero) 'the test process owns a native notification window'
  Assert-Proof ([AkmProofClipboard]::PostMessage($monitor, $message, [UIntPtr]$wparam, [IntPtr]::Zero)) 'synthetic notification was sent only to the test process'
}

function Get-ComboValue($combo) {
  $pattern = $null
  if ($combo.TryGetCurrentPattern([System.Windows.Automation.ValuePattern]::Pattern, [ref]$pattern)) { return $pattern.Current.Value }
  if ($combo.TryGetCurrentPattern([System.Windows.Automation.SelectionPattern]::Pattern, [ref]$pattern)) { return (@($pattern.Current.GetSelection() | ForEach-Object { $_.Current.Name }) -join '|') }
  return $null
}

# A view can load its options after it opens, so a selection made too early does not stick.
# Select, then confirm the combo box shows the value; retry up to 3 times.
function Select-Filter([string]$label, [string]$value) {
  for ($attempt = 1; $attempt -le 3; $attempt++) {
    Select-FilterOnce $label $value
    $combo = Find-Control $label ([System.Windows.Automation.ControlType]::ComboBox)
    $until = [DateTime]::UtcNow.AddSeconds(3)
    do {
      if ((Get-ComboValue $combo) -eq $value) { return }
      Start-Sleep -Milliseconds 100
    } while ([DateTime]::UtcNow -lt $until)
    Write-Output "RETRY: the $label selection did not show '$value' (attempt $attempt)"
  }
  throw "The $label combo box did not keep the value '$value' after 3 attempts"
}

function Get-OwnedProcessIds {
  $ownedIds = New-Object 'System.Collections.Generic.HashSet[int]'
  [void]$ownedIds.Add($process.Id)
  $queue = New-Object 'System.Collections.Generic.Queue[int]'
  $queue.Enqueue($process.Id)
  while ($queue.Count) {
    $parent = $queue.Dequeue()
    foreach ($child in (Get-CimInstance Win32_Process -Filter "ParentProcessId=$parent")) {
      if ($ownedIds.Add([int]$child.ProcessId)) { $queue.Enqueue([int]$child.ProcessId) }
    }
  }
  return ,$ownedIds
}

function Get-OwnedTopWindows($ownedIds) {
  foreach ($window in $AE::RootElement.FindAll($TS::Children, [System.Windows.Automation.Condition]::TrueCondition)) {
    $rootOwner = [AkmProofClipboard]::GetAncestor([IntPtr]$window.Current.NativeWindowHandle, 3)
    if (!$ownedIds.Contains($window.Current.ProcessId) -and $rootOwner -ne $process.MainWindowHandle) { continue }
    $window
  }
}

# Activates the enabled, visible option named $value in a window of the test process. Returns $true when it did.
function Invoke-FilterOption($ownedIds, $combo, [string]$value) {
  $condition = New-Object System.Windows.Automation.PropertyCondition($AE::NameProperty, $value)
  foreach ($window in @(Get-OwnedTopWindows $ownedIds)) {
    foreach ($item in $window.FindAll($TS::Descendants, $condition)) {
      if (!$item.Current.IsEnabled -or $item.Current.IsOffscreen) { continue }
      $action = $null
      if ($item.TryGetCurrentPattern([System.Windows.Automation.InvokePattern]::Pattern, [ref]$action)) {
        $action.Invoke()
        return $true
      }
      if ($item.TryGetCurrentPattern([System.Windows.Automation.SelectionItemPattern]::Pattern, [ref]$action)) {
        $action.Select()
        $combo.GetCurrentPattern([System.Windows.Automation.ExpandCollapsePattern]::Pattern).Collapse()
        return $true
      }
    }
  }
  return $false
}

function Get-FilterOptionNames($ownedIds) {
  $types = @([System.Windows.Automation.ControlType]::ListItem, [System.Windows.Automation.ControlType]::MenuItem, [System.Windows.Automation.ControlType]::DataItem)
  $condition = New-Object System.Windows.Automation.OrCondition(@($types | ForEach-Object { New-Object System.Windows.Automation.PropertyCondition($AE::ControlTypeProperty, $_) }))
  $names = New-Object 'System.Collections.Generic.List[string]'
  foreach ($window in @(Get-OwnedTopWindows $ownedIds)) {
    foreach ($item in $window.FindAll($TS::Descendants, $condition)) {
      if ($names.Count -ge 40) { break }
      $names.Add(('{0} (enabled={1}, offscreen={2})' -f $item.Current.Name, $item.Current.IsEnabled, $item.Current.IsOffscreen))
    }
  }
  return $names.ToArray()
}

# Turns the last stage of a failed filter step into a hint that separates a UI Automation race from an app failure.
function Get-FilterFailureHint([string]$stage, [string[]]$optionNames, [string]$value) {
  switch ($stage) {
    'combo-not-found' { return 'The control was never visible to UI Automation: the app is not in the expected view (app or navigation failure).' }
    'combo-unreadable' { return 'The combo box element became unavailable while it was read: the view re-rendered during the wait (a UI Automation race is likely).' }
    'combo-disabled' { return 'The combo box stayed disabled for the whole wait: the app did not enable it (app failure likely).' }
    'no-expand-pattern' { return 'The combo box existed but never exposed ExpandCollapse in the wait: a UI Automation race is likely.' }
    'expand-failed' { return 'The combo box exposed ExpandCollapse but every Expand call failed: a UI Automation race is likely.' }
    'option-missing' {
      if ($optionNames.Count -eq 0) { return 'The expanded list exposed no options to UI Automation: a UI Automation race or a popup that never rendered.' }
      if (@($optionNames | Where-Object { $_ -like "$value (*" }).Count -gt 0) { return 'The expected option is listed but was disabled, offscreen or not actionable.' }
      return 'Options were listed but the expected one is absent: the app did not offer this value (app failure likely).'
    }
    default { return 'No stage was recorded.' }
  }
}

function Write-FilterDiagnostic([string]$label, [string]$value, $combo, [string]$stage, [string]$detail, $ownedIds) {
  try {
    Write-Output "DIAG: filter step '$label' / '$value' failed at stage '$stage'. Last error: $detail"
    if ($combo) {
      $patterns = @($combo.GetSupportedPatterns() | ForEach-Object { $_.ProgrammaticName -replace 'PatternIdentifiers\.Pattern$', '' })
      Write-Output ('DIAG: combo box supported patterns: ' + $(if ($patterns.Count) { $patterns -join ', ' } else { '(none)' }))
      Write-Output ('DIAG: combo box enabled={0}, offscreen={1}, class={2}, automation id={3}' -f $combo.Current.IsEnabled, $combo.Current.IsOffscreen, $combo.Current.ClassName, $combo.Current.AutomationId)
      $pattern = $null
      if ($combo.TryGetCurrentPattern([System.Windows.Automation.ExpandCollapsePattern]::Pattern, [ref]$pattern)) {
        Write-Output "DIAG: combo box expand state: $($pattern.Current.ExpandCollapseState)"
      }
    } else {
      Write-Output 'DIAG: the combo box element was never found.'
    }
    $optionNames = @(Get-FilterOptionNames $ownedIds)
    Write-Output ('DIAG: option names found in windows of the test process: ' + $(if ($optionNames.Count) { $optionNames -join '; ' } else { '(none)' }))
    Write-Output ('DIAG: ' + (Get-FilterFailureHint $stage $optionNames $value))
  } catch {
    Write-Output "DIAG: collecting the diagnostic failed: $($_.Exception.Message)"
  }
}

# UI Automation can expose the combo box, its ExpandCollapse pattern and its options a little after the control appears.
# Wait for each with a bounded deadline, retry Expand, and re-open the list when the option does not show up.
function Select-FilterOnce([string]$label, [string]$value) {
  $deadline = [DateTime]::UtcNow.AddSeconds(15)
  $stage = 'combo-not-found'
  $detail = ''
  $combo = $null
  $ownedIds = Get-OwnedProcessIds
  while ([DateTime]::UtcNow -lt $deadline) {
    try {
      try {
        $combo = Find-Control $label ([System.Windows.Automation.ControlType]::ComboBox) 1000
      } catch {
        $stage = 'combo-not-found'; $detail = $_.Exception.Message
        continue
      }
      $stage = 'combo-unreadable'; $detail = 'the combo box element was not readable'
      $expand = $null
      if (!$combo.Current.IsEnabled) {
        $stage = 'combo-disabled'; $detail = 'the combo box is not enabled'
      } elseif (!$combo.TryGetCurrentPattern([System.Windows.Automation.ExpandCollapsePattern]::Pattern, [ref]$expand)) {
        $stage = 'no-expand-pattern'; $detail = 'ExpandCollapse is not exposed yet'
      } else {
        $stage = 'expand-failed'
        $expand.Expand()
        $stage = 'option-missing'; $detail = "no enabled option named '$value' was found"
        $ownedIds = Get-OwnedProcessIds
        $scanUntil = [DateTime]::UtcNow.AddSeconds(5)
        if ($scanUntil -gt $deadline) { $scanUntil = $deadline }
        do {
          if (Invoke-FilterOption $ownedIds $combo $value) { return }
          Start-Sleep -Milliseconds 100
        } while ([DateTime]::UtcNow -lt $scanUntil)
        try { $expand.Collapse() } catch { }
      }
    } catch {
      $detail = $_.Exception.Message
      if ($stage -eq 'option-missing') { $detail = "option scan failed: $detail" }
    }
    Start-Sleep -Milliseconds 100
  }
  Write-FilterDiagnostic $label $value $combo $stage $detail $ownedIds
  if ($stage -eq 'option-missing') { throw "Missing select option in the test process: $label / $value" }
  throw "Filter combo box did not become usable in the test process ($stage): $label / $value. Last error: $detail"
}

function Wait-SaveDialog {
  $condition = New-Object System.Windows.Automation.PropertyCondition($AE::ProcessIdProperty, $process.Id)
  $until = [DateTime]::UtcNow.AddSeconds(15)
  do {
    $windows = $AE::RootElement.FindAll($TS::Children, $condition)
    foreach ($window in $windows) {
      if ($window.Current.Name -eq 'Export plaintext environment file') { return $window }
      $dialog = $window.FindFirst($TS::Descendants, (New-Object System.Windows.Automation.PropertyCondition($AE::NameProperty, 'Export plaintext environment file')))
      if ($dialog) { return $dialog }
    }
    Start-Sleep -Milliseconds 100
  } while ([DateTime]::UtcNow -lt $until)
  throw 'Missing native AKM export dialog'
}

function Native-DialogControl([string]$id, [string]$name = '') {
  $condition = New-Object System.Windows.Automation.PropertyCondition($AE::AutomationIdProperty, $id)
  $until = [DateTime]::UtcNow.AddSeconds(12)
  do {
    foreach ($control in $script:root.FindAll($TS::Descendants, $condition)) {
      if (!$name -or $control.Current.Name -eq $name) { return $control }
    }
    Start-Sleep -Milliseconds 100
  } while ([DateTime]::UtcNow -lt $until)
  throw "Missing native dialog control $id / $name"
}

function Click-NativeDialog([string]$id, [string]$name) {
  $control = Native-DialogControl $id $name
  $pattern = $null
  if ($control.TryGetCurrentPattern([System.Windows.Automation.InvokePattern]::Pattern, [ref]$pattern)) { $pattern.Invoke() }
  else {
    $handle = Owned-NativeHandle $control ([int]$id)
    if (![AkmProofClipboard]::PostMessage($handle, 0x00F5, [UIntPtr]::Zero, [IntPtr]::Zero)) { throw "Could not click the test dialog's $name button" }
  }
}

function Native-FilenameControl {
  $condition = New-Object System.Windows.Automation.PropertyCondition($AE::AutomationIdProperty, 'FileNameControlHost')
  $hostControl = $script:root.FindFirst($TS::Descendants, $condition)
  if (!$hostControl) { throw 'Missing the native FileNameControlHost' }
  foreach ($control in $hostControl.FindAll($TS::Descendants, [System.Windows.Automation.Condition]::TrueCondition)) {
    if ($control.Current.AutomationId -eq '1001' -and $control.Current.ClassName -eq 'Edit') { return $control }
  }
  throw 'Missing the native filename Edit within FileNameControlHost'
}

function Owned-NativeHandle($control, [int]$id) {
  $handle = [IntPtr]$control.Current.NativeWindowHandle
  if ($handle -eq [IntPtr]::Zero) { $handle = [AkmProofClipboard]::GetDlgItem([IntPtr]$script:root.Current.NativeWindowHandle, $id) }
  [uint32]$ownerId = 0
  [void][AkmProofClipboard]::GetWindowThreadProcessId($handle, [ref]$ownerId)
  if ($handle -eq [IntPtr]::Zero -or $ownerId -ne $process.Id) { throw 'The native control does not belong to the test process' }
  return $handle
}

function Set-NativeDialogValue($control, [string]$value) {
  $pattern = $null
  if ($control.TryGetCurrentPattern([System.Windows.Automation.ValuePattern]::Pattern, [ref]$pattern)) {
    $pattern.SetValue($value)
    if ($pattern.Current.Value -cne $value) { throw 'The native filename did not accept the exact test path' }
  }
  else {
    $handle = [IntPtr]$control.Current.NativeWindowHandle
    [uint32]$ownerId = 0
    [void][AkmProofClipboard]::GetWindowThreadProcessId($handle, [ref]$ownerId)
    if ($handle -eq [IntPtr]::Zero -or $ownerId -ne $process.Id) { throw 'The native filename textbox does not belong to the test process' }
    if (![AkmProofClipboard]::SetVerifiedEditText($handle, $value)) { throw 'The native filename textbox did not retain the exact test path' }
  }
}

function Hold-TestJournal {
  # This sidecar belongs only to the temporary fake vault. Overlap fs2's byte-range lock.
  $stream = [IO.File]::Open(($vault + '.audit.jsonl.lock'), [IO.FileMode]::OpenOrCreate, [IO.FileAccess]::ReadWrite, [IO.FileShare]::ReadWrite)
  try { $stream.Lock(0, [Int64]::MaxValue) }
  catch { $stream.Dispose(); throw }
  return $stream
}

function Minimize-RestoreOwnedWindow {
  if (!$OwnerReadyForVisibility) { throw 'The owner has not confirmed readiness for native visibility changes.' }
  # Process.MainWindowHandle can switch to Tao's event-target window while the real window is minimized.
  $handle = [IntPtr]$script:root.Current.NativeWindowHandle
  $owned = $AE::FromHandle($handle)
  Assert-Proof ($owned.Current.ProcessId -eq $process.Id -and $owned.Current.ClassName -eq 'Tauri Window') 'visibility handoff targets only the owned AKM window'
  $window = $owned.GetCurrentPattern([System.Windows.Automation.WindowPattern]::Pattern)
  $window.SetWindowVisualState([System.Windows.Automation.WindowVisualState]::Minimized)
  try {
    $until = [DateTime]::UtcNow.AddSeconds(3)
    do {
      if ([AkmProofClipboard]::IsIconic($handle)) { break }
      Start-Sleep -Milliseconds 50
    } while ([DateTime]::UtcNow -lt $until)
    Assert-Proof ([AkmProofClipboard]::IsIconic($handle)) 'the owned AKM window actually minimized (native handle check)'
    # Allow its webview to process the visibility event, without global keystrokes or OS locking.
    Start-Sleep -Milliseconds 300
  } finally { $window.SetWindowVisualState([System.Windows.Automation.WindowVisualState]::Normal) }
  $script:root = $AE::FromHandle($handle)
}

# R4-04 checks. Each one reads complete snapshots of the owned window (tools/ProofSnapshot.psm1):
# text matches stay inside one element or one outcome record, a UI Automation error is never
# read as absence, and every sample also fails if a dismissed outcome has returned.
$OutcomeText = '(Import|File export|Operation) (is awaiting an outcome|completed\.|reported an error|outcome could not be confirmed)|File export was cancelled|its originating session is no longer available'
$FakeSecret = 'fake-prod-must-stay'

function ConvertFrom-CachedElement($element) {
  $children = @(foreach ($child in $element.CachedChildren) { ConvertFrom-CachedElement $child })
  [pscustomobject]@{
    Name     = $element.Cached.Name
    Role     = "$($element.Cached.LocalizedControlType)".ToLowerInvariant()
    Type     = $element.Cached.ControlType.ProgrammaticName.Replace('ControlType.', '').ToLowerInvariant()
    Enabled  = $element.Cached.IsEnabled
    Children = $children
  }
}

function Read-OwnedSnapshot([string]$sentinel) {
  if ($script:root.Current.ProcessId -ne $process.Id) { throw 'The UI Automation root no longer belongs to the proof process.' }
  $cached = $script:root.GetUpdatedCache($script:treeRequest)
  ConvertTo-ProofSnapshot (ConvertFrom-CachedElement $cached) $sentinel
}

function Wait-Owned([string]$sentinel, [scriptblock]$predicate, [int]$milliseconds = 15000) {
  Wait-ProofSnapshot -Reader { Read-OwnedSnapshot $sentinel } -Predicate $predicate -Milliseconds $milliseconds
}

function Assert-OwnedStable([string]$sentinel, [scriptblock]$predicate, [int]$milliseconds, [string]$description) {
  Assert-ProofStable -Reader { Read-OwnedSnapshot $sentinel } -Predicate $predicate -Milliseconds $milliseconds -Description $description
}

function Assert-GoneAndStable([string]$sentinel, [scriptblock]$predicate, [string]$description, [int]$stableMs = 2000) {
  Assert-Proof (Wait-Owned $sentinel $predicate 5000) $description
  Assert-OwnedStable $sentinel $predicate $stableMs "$description, and it stays so"
}

function Get-RevealCount {
  @(Get-Content -LiteralPath ($vault + '.audit.jsonl') | Where-Object { $_ } | ForEach-Object { ($_ | ConvertFrom-Json).action } | Where-Object { $_ -eq 'reveal-requested' }).Count
}

function Assert-OutcomeRegion([string]$pattern) {
  Assert-Proof (Wait-Owned 'Keys' { param($s) @(Get-ProofRecords $s $pattern).Count -ge 1 }) 'the application-owned classified outcome is visible after view handoff'
  $snapshot = Read-ProofSample { Read-OwnedSnapshot 'Keys' }
  Assert-Proof ($snapshot.Sentinel -and $snapshot.Region) 'the retained outcome belongs to the application-level region'
  Assert-Proof ($snapshot.Records.Count -eq 1 -and "$($snapshot.Records[0].Text)" -match $pattern) "exactly one application outcome record is retained, and it is the expected one (found $($snapshot.Records.Count))"
  $text = (@($snapshot.Records | ForEach-Object { $_.Text }) -join ' ')
  foreach ($private in @($work, 'HANDOFF_TOKEN', 'fake-outcome-import-value', 'outcome-import-project', 'outcome-import-env', 'handoff-export.env')) {
    Assert-Proof (!$text.Contains($private)) 'the retained outcome contains no fake protected path, input or scope'
  }
}

# Dismisses exactly the expected record(s), waits for the removal, and only then adds the
# pattern to the dismissed list, so a correct but slow removal never fails the proof.
function Dismiss-Outcome([string]$pattern, [int]$count = 1) {
  $snapshot = Read-ProofSample { Read-OwnedSnapshot 'Keys' }
  Assert-Proof ($snapshot.Sentinel -and $snapshot.Records.Count -eq $count -and @(Get-ProofRecords $snapshot $pattern).Count -eq $count) "just before dismissal, the region holds exactly $count expected outcome record(s)"
  Click-Control 'Dismiss operation outcomes'
  Assert-Proof (Wait-Owned 'Keys' { param($s) !$s.Region } 5000) 'the dismissed outcome region is removed'
  Add-ProofDismissed $pattern
  Assert-OwnedStable 'Keys' { param($s) !$s.Region } 2000 'the dismissed outcome stays absent through later polls'
}

function Test-KeyDetailExpiry([switch]$Rehearse) {
  $shown = { param($s) Test-ProofText $s $FakeSecret }
  $masked = { param($s)
    !(Test-ProofText $s $FakeSecret) -and
    @($s.Dialog | Where-Object { $_.Name -ceq 'Reveal' -and $_.Type -eq 'button' -and $_.Enabled }).Count -eq 1 -and
    @($s.Dialog | Where-Object { $_.Role -eq 'alert' }).Count -eq 0
  }
  Click-Control 'Keys'
  Click-Control 'Open OpenAI prod proof'

  # 1. A displayed secret.
  Click-Control 'Reveal'
  Assert-Proof (Wait-Owned 'Copy' $shown) 'Key Detail reveals the fake secret'
  if ($Rehearse) { Click-Control 'Hide'; $how = 'rehearsal: Hide' }
  else { Minimize-RestoreOwnedWindow; $how = 'actual minimize/restore' }
  Assert-Proof (Wait-Owned 'Copy' $masked 5000) "$how masks the displayed fake secret, and Key Detail can reveal again"
  Assert-OwnedStable 'Copy' $masked 5000 "Key Detail stays open and masked after $how"

  # 2. A reveal that is still pending at the hide. The test-owned journal lock delays it.
  $before = Get-RevealCount
  $hold = Hold-TestJournal
  try {
    Click-Control 'Reveal'
    Assert-Proof (Wait-Owned 'Copy' { param($s) @($s.Dialog | Where-Object { $_.Name -ceq ('Revealing' + [char]0x2026) }).Count -eq 1 } 5000) 'the fake reveal is pending on the test-owned journal lock'
    Assert-OwnedStable 'Copy' { param($s) !(Test-ProofText $s $FakeSecret) } 1000 'the pending fake reveal discloses nothing while the lock is held'
    if (!$Rehearse) { Minimize-RestoreOwnedWindow }
  } finally { $hold.Unlock(0, [Int64]::MaxValue); $hold.Dispose() }
  if ($Rehearse) {
    Assert-Proof (Wait-Owned 'Copy' $shown) 'rehearsal: the delayed fake reveal completes after the lock is released, so the delay works'
    Click-Control 'Hide'
    Assert-Proof (Wait-Owned 'Copy' $masked 5000) 'rehearsal: Hide masks the delayed fake secret'
  } else {
    Assert-Proof (Wait-Owned 'Copy' $masked 5000) 'after minimize/restore, the pending reveal leaves Key Detail masked and ready'
    Assert-OwnedStable 'Copy' $masked 5000 'Key Detail stays open, and the late reveal result shows neither the fake secret nor a stale error'
  }
  $until = [DateTime]::UtcNow.AddSeconds(5)
  while ((Get-RevealCount) -lt ($before + 1) -and [DateTime]::UtcNow -lt $until) { Start-Sleep -Milliseconds 100 }
  Assert-Proof ((Get-RevealCount) -eq ($before + 1)) 'the native side completed the delayed reveal request during the observation'

  Click-Control 'Close'
  Assert-GoneAndStable 'Keys' { param($s) $s.Dialog.Count -eq 0 -and !(Test-ProofText $s $FakeSecret) } 'the closed Key Detail leaves no fake secret on screen'
}

function Test-CancelledOutcome([int]$Count = 1) {
  $pattern = 'File export was cancelled\. No file was saved by this request\.'
  Click-Control 'Settings'
  Select-Filter 'Project' 'demo'
  Select-Filter 'Environment' 'prod'
  for ($i = 1; $i -le $Count; $i++) {
    Click-Control ('Save .env' + [char]0x2026)
    $mainRoot = $script:root
    $script:root = Wait-SaveDialog
    Click-NativeDialog '2' 'Cancel'
    $script:root = $mainRoot
    Assert-Proof (Wait-Owned 'Keys' { param($s) @(Get-ProofRecords $s $pattern).Count -eq $i }) "native Cancel $i retains its own cancellation record"
  }
  if ($Count -eq 1) { Assert-OutcomeRegion $pattern }
  else {
    $snapshot = Read-ProofSample { Read-OwnedSnapshot 'Keys' }
    Assert-Proof ($snapshot.Records.Count -eq $Count) "the record count sees $Count separate outcome records in the real window (found $($snapshot.Records.Count))"
    $rejected = $false
    try { Assert-OutcomeRegion $pattern } catch { $rejected = $_.Exception.Message -match 'exactly one application outcome record' }
    Assert-Proof $rejected "the exactly-one check rejects $Count retained records"
  }
  Dismiss-Outcome 'File export was cancelled' $Count
}

function Assert-NoOutcomeResurrection {
  Assert-OwnedStable 'Keys' { param($s) !$s.Region -and !(Test-ProofText $s $OutcomeText) } 3500 'every dismissed outcome (success, cancellation, error, pending, unconfirmed, completed while locked) stays absent through later status and warning polls'
}

function Test-OutcomeHandoff {
  Test-KeyDetailExpiry
  Click-Control 'Settings'
  Click-Control ('Import .env' + [char]0x2026)
  Set-Field 'Paste the .env contents' 'HANDOFF_TOKEN=fake-outcome-import-value'
  Set-Field 'Target project' 'outcome-import-project'
  Set-Field 'Target environment' 'outcome-import-env'
  Click-Control 'Review import'
  Find-Control 'Confirm import' ([System.Windows.Automation.ControlType]::Button) | Out-Null
  $before = (Get-FileHash -LiteralPath $vault -Algorithm SHA256).Hash
  $hold = Hold-TestJournal
  try {
    Click-Control 'Confirm import'
    Assert-Proof (Wait-Owned 'Keys' { param($s) @(Get-ProofRecords $s 'Import is awaiting an outcome').Count -eq 1 }) 'dispatched import creates a pending application-owned outcome'
    $until = [DateTime]::UtcNow.AddSeconds(20)
    do {
      $changed = (Get-FileHash -LiteralPath $vault -Algorithm SHA256).Hash -ne $before
      if (!$changed) { Start-Sleep -Milliseconds 100 }
    } while (!$changed -and [DateTime]::UtcNow -lt $until)
    Assert-Proof $changed 'the fake encrypted import commits while its reply waits on the test-owned journal lock'
    Minimize-RestoreOwnedWindow
    Assert-GoneAndStable 'Keys' { param($s) !(Test-ProofText $s '^Import keys$') } 'actual minimize/restore expired the local import dialog'
    Assert-Proof (Find-Control ('Import .env' + [char]0x2026) ([System.Windows.Automation.ControlType]::Button)).Current.IsEnabled 'the hidden confirmation does not leave Settings busy'
    Click-Control 'Keys'
  } finally { $hold.Unlock(0, [Int64]::MaxValue); $hold.Dispose() }
  Assert-OutcomeRegion 'Import completed\.'
  Assert-OwnedStable 'Keys' { param($s) !(Test-ProofText $s 'Imported: 1 added') } 2000 'obsolete Settings details do not reappear on Keys'
  $keys = Invoke-TestCli @('list', '--json') | Out-String | ConvertFrom-Json
  Assert-Proof (@($keys | Where-Object { $_.env_var -eq 'HANDOFF_TOKEN' -and $_.project -eq 'outcome-import-project' -and $_.environment -eq 'outcome-import-env' }).Count -eq 1) 'the actual CLI confirms the dispatched import saved once in its reviewed scope'
  Dismiss-Outcome 'Import completed\.'

  Click-Control 'Settings'
  Select-Filter 'Project' 'demo'
  Select-Filter 'Environment' 'prod'
  $hold = Hold-TestJournal
  try {
    Click-Control ('Save .env' + [char]0x2026)
    $mainRoot = $script:root
    $script:root = Wait-SaveDialog
    $exportPath = Join-Path $work 'handoff-export.env'
    Set-NativeDialogValue (Native-FilenameControl) $exportPath
    Click-NativeDialog '1' 'Save'
    $script:root = $mainRoot
    Assert-Proof (Wait-Owned 'Keys' { param($s) @(Get-ProofRecords $s 'File export is awaiting an outcome').Count -eq 1 }) 'native-dialog file save has a pending application-owned outcome'
    Minimize-RestoreOwnedWindow
    Assert-Proof (Find-Control ('Save .env' + [char]0x2026) ([System.Windows.Automation.ControlType]::Button)).Current.IsEnabled 'actual minimize/restore expired the local file-save continuation'
    Click-Control 'Keys'
  } finally { $hold.Unlock(0, [Int64]::MaxValue); $hold.Dispose() }
  Assert-OutcomeRegion 'File export completed\.'
  Assert-Proof ([IO.File]::ReadAllText($exportPath) -eq "OPENAI_API_KEY=fake-prod-must-stay`n") 'handoff export published exactly the selected fake scope to the approved destination'
  $savedTo = 'Saved plaintext secrets to ' + [regex]::Escape($exportPath)
  Assert-OwnedStable 'Keys' { param($s) !(Test-ProofText $s $savedTo) } 2000 'the export destination is not replayed outside its originating Settings view'
  Dismiss-Outcome 'File export completed\.'

  Test-CancelledOutcome

  # Delay a real pre-disclosure error too; an error outcome must not claim nothing was saved.
  $journal = $vault + '.audit.jsonl'
  $journalBefore = [IO.File]::ReadAllBytes($journal)
  $hold = Hold-TestJournal
  try {
    Remove-Item -LiteralPath $journal
    New-Item -ItemType Directory -Path $journal | Out-Null
    Click-Control ('Save .env' + [char]0x2026)
    $mainRoot = $script:root
    $script:root = Wait-SaveDialog
    $refusedPath = Join-Path $work 'handoff-refused.env'
    Set-NativeDialogValue (Native-FilenameControl) $refusedPath
    Click-NativeDialog '1' 'Save'
    $script:root = $mainRoot
    Minimize-RestoreOwnedWindow
    Click-Control 'Keys'
  } finally { $hold.Unlock(0, [Int64]::MaxValue); $hold.Dispose() }
  try {
    Assert-OutcomeRegion 'File export reported an error\. This does not confirm that nothing was saved\.'
    Assert-Proof (!(Test-Path -LiteralPath $refusedPath)) 'the induced request-journal failure did not publish a plaintext file'
  } finally { Remove-Item -LiteralPath $journal; [IO.File]::WriteAllBytes($journal, $journalBefore) }
  Dismiss-Outcome 'File export reported an error'
  Assert-NoOutcomeResurrection
}

try {
  if (Get-Process -Name 'akm-desktop' -ErrorAction SilentlyContinue) { throw 'Another AKM process is open. Close it yourself before running the owned-process proof.' }
  if (!$SkipClipboard) { Assert-EmptyClipboard }
  Write-Output ([AkmProofClipboard]::DesktopDescription())
  if ($RemainingOnly -or $JournalOnly) {
    Invoke-TestCli @('init', '--name', 'Remaining native proof') | Out-Null
    Invoke-TestCli @('add', 'OpenAI prod proof', '--project', 'demo', '--env', 'prod', '--env-var', 'OPENAI_API_KEY', '--secret', 'fake-prod-must-stay') | Out-Null
    if ($JournalOnly) {
      $vault = $secondVault
      Invoke-TestCli @('init', '--name', 'Second fake vault') | Out-Null
      $vault = $mainVault
    }
  } elseif ($KeePassXC) {
    python (Join-Path $PSScriptRoot 'prepare_gui_fixture.py') --akm $cli --keepassxc $KeePassXC --directory $work
    if ($LASTEXITCODE -ne 0) { throw 'Could not prepare the fake GUI clone fixture' }
  }
  $process = Start-Process -FilePath $desktop -PassThru
  Write-Output "Proof process ID: $($process.Id)"
  $recovery.processId = $process.Id
  $recovery.processStartUtc = $process.StartTime.ToUniversalTime().ToString('o')
  Write-Output "Proof process start UTC: $($recovery.processStartUtc)"
  $recovery | ConvertTo-Json | Set-Content -LiteralPath (Join-Path $work 'recovery.json') -Encoding UTF8
  $until = [DateTime]::UtcNow.AddSeconds(20)
  do {
    $process.Refresh()
    if ($process.MainWindowHandle -ne [IntPtr]::Zero) { $script:root = $AE::FromHandle($process.MainWindowHandle) }
    if (!$script:root) { Start-Sleep -Milliseconds 100 }
  } while (!$script:root -and [DateTime]::UtcNow -lt $until)
  Assert-Proof ($null -ne $script:root) 'a fresh AKM window opened'

  try { Find-Control 'Vault file' ([System.Windows.Automation.ControlType]::Edit) 30000 | Out-Null }
  catch {
    Save-ProofDiagnostics 'startup-controls-missing'
    # A failed probe's full traversal has exposed normal controls in saved WebView diagnostics.
    # Reacquire only this process's root and check the expected control before assuming an app error.
    $process.Refresh()
    $script:root = $AE::FromHandle($process.MainWindowHandle)
    try {
      Find-Control 'Vault file' ([System.Windows.Automation.ControlType]::Edit) 5000 | Out-Null
      Write-Output 'PASS: startup controls became accessible after refreshing the owned UI Automation root'
    } catch {
      try { Find-Control 'Try again' ([System.Windows.Automation.ControlType]::Button) 500 | Out-Null }
      catch { throw 'Startup controls are not accessible. Inspect native-diagnostics.txt before retrying.' }
      Click-Control 'Try again'
      Find-Control 'Vault file' ([System.Windows.Automation.ControlType]::Edit) 30000 | Out-Null
      Write-Output 'PASS: startup recovered through its visible retry control'
    }
  }
  Set-Field 'Vault file' $vault
  if ($RemainingOnly -or $JournalOnly) {
    try { Find-Control 'Unlock vault' ([System.Windows.Automation.ControlType]::Button) 500 | Out-Null }
    catch { Click-Control 'I already have a vault' }
    Unlock-FakeVault
    if ($JournalOnly) {
      if ($OutcomeRehearsal) {
        Test-KeyDetailExpiry -Rehearse
        Test-CancelledOutcome -Count 2
        Assert-NoOutcomeResurrection
        $script:root.GetCurrentPattern([System.Windows.Automation.WindowPattern]::Pattern).Close()
        Assert-Proof ($process.WaitForExit(10000)) 'the outcome rehearsal exited normally'
        Write-Output 'REHEARSAL ONLY: no minimize, clipboard or OS signal. This does not pass R4-04.'
        $success = $true
        return
      }
      if ($OutcomeOnly) {
        Test-OutcomeHandoff
        $script:root.GetCurrentPattern([System.Windows.Automation.WindowPattern]::Pattern).Close()
        Assert-Proof ($process.WaitForExit(10000)) 'the focused outcome proof exited normally'
        Write-Output 'SKIPPED: clipboard, OS lock/suspend, Tauri reload and unrelated native flows; no clipboard access was performed.'
        $success = $true
        return
      }
      if ($JournalExportOnly) {
        $encryptedBefore = [Convert]::ToBase64String([IO.File]::ReadAllBytes($vault))
        Click-Control 'Settings'
        Select-Filter 'Project' 'demo'
        Select-Filter 'Environment' 'prod'
        Click-Control ('Save .env' + [char]0x2026)
        $mainRoot = $script:root
        $script:root = Wait-SaveDialog
        $exportPath = Join-Path $work 'journal-export.env'
        Set-NativeDialogValue (Native-FilenameControl) $exportPath
        Click-NativeDialog '1' 'Save'
        $script:root = $mainRoot
        Assert-Proof (Wait-Text ('Saved plaintext secrets to ' + [regex]::Escape($exportPath))) 'the rebuilt export published to the exact approved native destination'
        Assert-Proof ([IO.File]::ReadAllText($exportPath) -eq "OPENAI_API_KEY=fake-prod-must-stay`n") 'the journal export contains only its selected fake prod secret'
        $journal = $vault + '.audit.jsonl'
        $journalBefore = [IO.File]::ReadAllBytes($journal)
        $actions = @(Get-Content -LiteralPath $journal | ForEach-Object { ($_ | ConvertFrom-Json).action })
        Assert-Proof ($actions[-2] -eq 'export-requested' -and $actions[-1] -eq 'export-completed') 'native export records a request before its known publication completion'
        Assert-Proof ([Convert]::ToBase64String([IO.File]::ReadAllBytes($vault)) -eq $encryptedBefore) 'export journaling did not mutate the encrypted vault'
        Remove-Item -LiteralPath $journal
        New-Item -ItemType Directory -Path $journal | Out-Null
        Click-Control ('Save .env' + [char]0x2026)
        $script:root = Wait-SaveDialog
        $refusedPath = Join-Path $work 'must-not-publish.env'
        Set-NativeDialogValue (Native-FilenameControl) $refusedPath
        Click-NativeDialog '1' 'Save'
        $script:root = $mainRoot
        Assert-Proof (Wait-Text 'Access is denied|permission denied|I/O error') 'failed pre-disclosure journaling is visibly refused'
        Assert-Proof (!(Test-Path -LiteralPath $refusedPath)) 'a native export whose request cannot be recorded publishes no plaintext file'
        Remove-Item -LiteralPath $journal
        [IO.File]::WriteAllBytes($journal, $journalBefore)
        $script:root.GetCurrentPattern([System.Windows.Automation.WindowPattern]::Pattern).Close()
        Assert-Proof ($process.WaitForExit(10000)) 'the focused export journal proof exited normally'
        Write-Output 'SKIPPED: clipboard and unrelated native flows; no clipboard access was performed.'
        $success = $true
        return
      }
      Click-Control 'Activity'
      Assert-Proof (Wait-Text 'Recorded chain is consistent') 'the real GUI reads a consistent journal snapshot'
      Assert-Proof (Wait-Text 'Completeness unknown') 'the real GUI does not claim a complete activity history'
      Assert-Proof (Wait-Text 'Added') 'the real GUI presents a known completed mutation'
      $journal = $vault + '.audit.jsonl'
      $legacyBackup = $vault + '.bak'
      Remove-Item -LiteralPath $legacyBackup
      New-Item -ItemType Directory -Path $legacyBackup | Out-Null
      Click-Control 'Keys'
      Click-Control 'Open OpenAI prod proof'
      Click-Control 'Delete key'
      $beforeDelete = (Get-FileHash -LiteralPath $vault -Algorithm SHA256).Hash
      Assert-Proof (Wait-Text 'recovery depends on their contents and retention') 'delete confirmation does not promise recovery regardless of maintenance outcomes'
      Click-Control 'Delete key'
      Assert-Proof (Wait-Text 'Check backup notices and your encrypted backups before relying on recovery') 'successful deletion does not unconditionally promise retained recovery'
      Assert-Proof (Wait-Text 'convenience backup could not be refreshed') 'the actual failed backup refresh appears separately from deletion success'
      $exactOriginal = @(Get-ChildItem -LiteralPath $work -Filter 'vault.kdbx.bak.*.kdbx' -File | Where-Object { (Get-FileHash -LiteralPath $_.FullName -Algorithm SHA256).Hash -eq $beforeDelete })
      Assert-Proof ($exactOriginal.Count -ge 1) 'the exact immediate pre-deletion encrypted original remains, not merely an arbitrary dated file'
      $deletedKeys = Invoke-TestCli @('list', '--json') | Out-String | ConvertFrom-Json
      Assert-Proof (@($deletedKeys | Where-Object { $_.title -eq 'OpenAI prod proof' }).Count -eq 0) 'the CLI confirms the deletion despite the convenience-backup failure'
      $activeProofVault = $vault
      try {
        $vault = $exactOriginal[0].FullName
        $recovered = (Invoke-TestCli @('get', 'OpenAI prod proof', '--field', 'secret') | Out-String).Trim()
        Assert-Proof ($recovered -ceq 'fake-prod-must-stay') 'the exact immediate original still decrypts and recovers the deleted fake key through the actual CLI'
      } finally { $vault = $activeProofVault }
      Click-Control 'Dismiss saved-operation notices'
      Remove-Item -LiteralPath $legacyBackup
      Click-Control 'Activity'
      # Valid unknown-action fixture using the journal's existing public hash encoding.
      $sha = [Security.Cryptography.SHA256]::Create()
      $previous = ''
      $sequence = 0
      $fixtureLines = @()
      foreach ($action in @('__proto__', 'constructor', 'toString')) {
        $sequence++
        $timestamp = '2026-10-31T00:00:00+00:00'
        $payload = @($previous, [string]$sequence, $timestamp, $action, '', '', '', '', '') -join "`n"
        $hash = [BitConverter]::ToString($sha.ComputeHash([Text.Encoding]::UTF8.GetBytes($payload))).Replace('-', '').ToLowerInvariant()
        $fixtureLines += ([ordered]@{seq=$sequence;ts=$timestamp;action=$action;prev=$previous;hash=$hash} | ConvertTo-Json -Compress)
        $previous = $hash
      }
      $sha.Dispose()
      [IO.File]::WriteAllText($journal, (($fixtureLines -join "`n") + "`n"), [Text.UTF8Encoding]::new($false))
      Click-Control 'Refresh'
      Assert-Proof (Wait-Text 'Recorded chain is consistent') 'unknown actions retain a consistent native journal snapshot'
      foreach ($action in @('__proto__', 'constructor', 'toString')) {
        Assert-Proof (Wait-Text ([regex]::Escape("Legacy $action (outcome unknown)"))) "the real Activity view safely displays literal unknown action $action"
      }
      [IO.File]::WriteAllText($journal, '{"seq":9007199254740993,"ts":"2026-10-31T00:00:00Z","action":"fake","prev":"","hash":"bad"}' + "`n")
      Click-Control 'Refresh'
      Assert-Proof (Wait-Text '9007199254740993') 'the native IPC and real Activity view preserve an offending u64 sequence exactly'
      Assert-Proof (!(Wait-Text '9007199254740992' 300)) 'the GUI does not display the rounded JavaScript-number value'
      [IO.File]::WriteAllText($journal, '{"fake-damaged-record":')
      $damaged = [Convert]::ToBase64String([IO.File]::ReadAllBytes($journal))
      Click-Control 'Refresh'
      Assert-Proof (Wait-Text 'Journal data is damaged') 'malformed journal bytes produce an explicit damaged state'
      Assert-Proof ([Convert]::ToBase64String([IO.File]::ReadAllBytes($journal)) -eq $damaged) 'reading a damaged journal leaves its original bytes unchanged'
      Remove-Item -LiteralPath $journal
      Click-Control 'Refresh'
      Assert-Proof (Wait-Text 'Journal unavailable') 'a missing journal is unavailable, not verified or proof of inactivity'
      New-Item -ItemType Directory -Path $journal | Out-Null
      Click-Control 'Keys'
      Click-Control 'Add key'
      Set-Field 'Title' 'GUI journal warning proof'
      Set-Field 'Secret' 'fake-saved-without-journal'
      Click-Control 'Add key'
      Assert-Proof (Wait-Text 'A vault change was saved, but its activity entry could not be confirmed') 'a committed write with uncertain journaling has a non-retryable operation notice'
      $keys = Invoke-TestCli @('list', '--json') | Out-String | ConvertFrom-Json
      Assert-Proof (@($keys | Where-Object { $_.title -eq 'GUI journal warning proof' }).Count -eq 1) 'the actual CLI sees the GUI mutation saved exactly once'
      Click-Control 'Lock now'
      $vault = $secondVault
      Set-Field 'Vault file' $vault
      Unlock-FakeVault
      Assert-Proof (Wait-Text 'A vault change was saved, but its activity entry could not be confirmed') 'the earlier operation acknowledgment survives opening another vault'
      Click-Control 'Activity'
      Assert-Proof (Wait-Text 'Journal unavailable') 'the second vault retains its own journal state'
      Assert-Proof (!(Wait-Text 'GUI journal warning proof' 300)) 'the second vault does not display first-vault journal metadata'
      Click-Control 'Dismiss saved-operation notices'
      Assert-Proof (!(Wait-Text 'Could not dismiss these notices' 500)) 'the registered native acknowledgment command accepted displayed notice IDs'
      Assert-Proof (!(Wait-Text 'activity entry could not be confirmed' 4000)) 'explicitly dismissed notices remain absent across subsequent native polling'
      $script:root.GetCurrentPattern([System.Windows.Automation.WindowPattern]::Pattern).Close()
      Assert-Proof ($process.WaitForExit(10000)) 'the focused journal proof app exited normally'
      Write-Output 'SKIPPED: clipboard, OS lock/suspend and unrelated native flows; no clipboard access was performed.'
      $success = $true
      return
    }
    Write-Output 'RESUMED: only native save, clipboard, session/power, exit and affinity checks; prior GUI checks were not repeated.'
  } else {
  try { Find-Control 'Create vault' ([System.Windows.Automation.ControlType]::Button) 500 | Out-Null }
  catch { Click-Control 'Create a new vault instead' }
  Set-Field 'Master password' $password
  Set-Field 'Confirm password' $password
  Click-Control 'Create vault'
  Find-Control 'Add key' ([System.Windows.Automation.ControlType]::Button) | Out-Null
  Assert-Proof (Test-Path -LiteralPath $vault) 'the GUI created its fake vault'

  Invoke-TestCli @('add', 'OpenAI prod proof', '--project', 'demo', '--env', 'prod', '--env-var', 'OPENAI_API_KEY', '--secret', 'fake-prod-must-stay') | Out-Null
  Click-Control 'Add key'
  Set-Field 'Title' 'Phantom must not exist'
  Set-Field 'Secret' 'fake-unsaved-value'
  Click-Control 'Add key'
  Assert-Proof (Wait-Text 'changed on disk') 'stale GUI write was refused'
  Click-Control 'Cancel'
  Click-Control 'Activity'
  Click-Control 'Keys'
  Assert-Proof (Wait-Text 'No keys yet\.') 'failed write did not mutate the open snapshot'
  Click-Control 'Reload vault'
  Find-Control 'Open OpenAI prod proof' ([System.Windows.Automation.ControlType]::Button) | Out-Null

  Click-Control 'Settings'
  $beforeInvalid = [Convert]::ToBase64String([IO.File]::ReadAllBytes($vault))
  foreach ($inputText in @('TOKEN=', 'bad-name=fake', "TOKEN=fake`nTOKEN=other")) {
    Click-Control ('Import .env' + [char]0x2026)
    Set-Field 'Paste the .env contents' $inputText
    Set-Field 'Target project' 'demo'
    Set-Field 'Target environment' 'dev'
    Click-Control 'Review import'
    Assert-Proof (Wait-Text 'must have a non-empty|not a valid environment variable|appears more than once') 'the GUI rejected an empty, invalid or duplicate import'
    Assert-Proof ([Convert]::ToBase64String([IO.File]::ReadAllBytes($vault)) -eq $beforeInvalid) 'a rejected import did not change the encrypted vault'
    Click-Control 'Cancel'
  }
  Click-Control ('Import .env' + [char]0x2026)
  Set-Field 'Paste the .env contents' 'OPENAI_API_KEY=fake-dev-value'
  Set-Field 'Target project' 'demo'
  Set-Field 'Target environment' 'dev'
  Click-Control 'Review import'
  Find-Control 'Confirm import' ([System.Windows.Automation.ControlType]::Button) | Out-Null
  Assert-Proof (Wait-Text '1\s*new,\s*0\s*replacements') 'dev import review does not target the prod key'
  $beforeConfirm = Invoke-TestCli @('list', '--json') | Out-String | ConvertFrom-Json
  Assert-Proof ($beforeConfirm.Count -eq 1) 'preview did not write to disk'
  Click-Control 'Confirm import'
  Assert-Proof (Wait-Text 'Imported: 1 added, 0 updated\.') 'confirmed scoped import saved'
  $keys = Invoke-TestCli @('list', '--json') | Out-String | ConvertFrom-Json
  $prod = $keys | Where-Object { $_.environment -eq 'prod' }
  $actual = Invoke-TestCli @('get', $prod.id, '--field', 'secret') | Out-String
  Assert-Proof ($actual.Trim() -eq 'fake-prod-must-stay') 'prod secret stayed unchanged'

  if ($KeePassXC) {
    Click-Control 'Lock now'
    $vault = $cloneVault
    Set-Field 'Vault file' $vault
    Set-Field 'Master password' 'fake-deliberately-wrong-password'
    Click-Control 'Unlock vault'
    Assert-Proof (Wait-Text 'That password does not open this vault') 'a failed GUI unlock retains its authentication error'
    Unlock-FakeVault
    Click-Control 'Open Clone target proof'
    Click-Control 'Edit'
    Set-Field 'Secret (optional)' 'fake-clone-edited'
    Click-Control 'Save changes'
    Assert-Proof (Wait-Text 'Saved.*Clone target proof') 'the GUI completed the clone edit'
    $keys = Invoke-TestCli @('list', '--json') | Out-String | ConvertFrom-Json
    $original = $keys | Where-Object { $_.title -eq 'Clone original proof' }
    $clone = $keys | Where-Object { $_.title -eq 'Clone target proof' }
    Assert-Proof ($original.id -ne $clone.id) 'cloned AKM_ID entries have distinct native identities'
    $originalSecret = Invoke-TestCli @('get', $original.id, '--field', 'secret') | Out-String
    $cloneSecret = Invoke-TestCli @('get', $clone.id, '--field', 'secret') | Out-String
    Assert-Proof ($originalSecret.Trim() -eq 'fake-original-kept' -and $cloneSecret.Trim() -eq 'fake-clone-edited') 'the GUI edited the clone without modifying its original'
  }

  Click-Control 'Lock now'
  Copy-Item -LiteralPath (Join-Path $PSScriptRoot '..\crates\akm-core\tests\fixtures\kdbx40.kdbx') -Destination $legacyVault
  $vault = $legacyVault
  $password = 'correct horse battery staple'
  $legacyBefore = [Convert]::ToBase64String([IO.File]::ReadAllBytes($vault))
  Set-Field 'Vault file' $vault
  Unlock-FakeVault
  Assert-Proof (!(Find-Control 'Add key' ([System.Windows.Automation.ControlType]::Button)).Current.IsEnabled) 'an older vault opens read-only in the GUI'
  Click-Control 'Review format upgrade'
  Assert-Proof ([Convert]::ToBase64String([IO.File]::ReadAllBytes($vault)) -eq $legacyBefore) 'opening the upgrade confirmation does not write'
  Click-Control 'Cancel'
  Assert-Proof ([Convert]::ToBase64String([IO.File]::ReadAllBytes($vault)) -eq $legacyBefore) 'cancelling upgrade preserves the original encrypted file'
  Click-Control 'Review format upgrade'
  Click-Control 'Upgrade format'
  $until = [DateTime]::UtcNow.AddSeconds(20)
  do {
    $enabled = (Find-Control 'Add key' ([System.Windows.Automation.ControlType]::Button)).Current.IsEnabled
    if (!$enabled) { Start-Sleep -Milliseconds 100 }
  } while (!$enabled -and [DateTime]::UtcNow -lt $until)
  Assert-Proof $enabled 'confirming the GUI upgrade enabled writes'
  Assert-Proof ([BitConverter]::ToUInt32([IO.File]::ReadAllBytes($vault), 8) -eq 0x00040001) 'the GUI explicitly upgraded the format to KDBX 4.1'
  Assert-Proof ([Convert]::ToBase64String([IO.File]::ReadAllBytes($vault + '.bak')) -eq $legacyBefore) 'the GUI upgrade retained the encrypted original backup'

  Click-Control 'Lock now'
  $vault = $mainVault
  $password = $mainPassword
  Set-Field 'Vault file' $vault
  Find-Control 'Unlock vault' ([System.Windows.Automation.ControlType]::Button) | Out-Null
  Unlock-FakeVault
  }
  Click-Control 'Settings'
  Select-Filter 'Project' 'demo'
  Select-Filter 'Environment' 'prod'
  Click-Control ('Save .env' + [char]0x2026)
  $mainRoot = $script:root
  $script:root = Wait-SaveDialog
  Click-NativeDialog '2' 'Cancel'
  $script:root = $mainRoot
  Find-Control ('Save .env' + [char]0x2026) ([System.Windows.Automation.ControlType]::Button) | Out-Null
  Assert-Proof (!(Test-Path -LiteralPath (Join-Path $work 'native-export.env'))) 'cancelling the native save dialog wrote no export'
  Click-Control ('Save .env' + [char]0x2026)
  $script:root = Wait-SaveDialog
  $filename = Native-FilenameControl
  $exportPath = Join-Path $work 'native-export.env'
  Set-NativeDialogValue $filename $exportPath
  Click-NativeDialog '1' 'Save'
  $script:root = $mainRoot
  Assert-Proof (Wait-Text ('Saved plaintext secrets to ' + [regex]::Escape($exportPath))) 'the native save dialog completed an export to the exact test destination'
  Assert-Proof ([IO.File]::ReadAllText($exportPath) -eq "OPENAI_API_KEY=fake-prod-must-stay`n") 'the native export contains only the selected prod value'
  Click-Control 'Keys'
  if (!$SkipClipboard) {
  Click-Control 'Open OpenAI prod proof'
  Assert-EmptyClipboard
  Click-Control 'Copy'
  Assert-Proof (Wait-Text 'Copied\.') 'native copy completed'
  $clipboardCheck = @(Check-ClipboardExclusions)
  $sequence = $clipboardCheck[$clipboardCheck.Count - 1]
  $clipboardCheck | Where-Object { $_ -is [string] } | Write-Output
  }
  Send-TestSignal 0x02B1 7
  Find-Control 'Unlock vault' ([System.Windows.Automation.ControlType]::Button) | Out-Null
  if (!$SkipClipboard) { Assert-Proof (!(Get-Clipboard)) 'session-lock notification cleared the owned clipboard copy' }
  Send-TestSignal 0x02B1 8
  Unlock-FakeVault
  if (!$SkipClipboard) {
  Click-Control 'Open OpenAI prod proof'
  Assert-EmptyClipboard
  Click-Control 'Copy'
  Assert-Proof (Wait-Text 'Copied\.') 'copy completed before the suspend check'
  }
  Send-TestSignal 0x0218 4
  Find-Control 'Unlock vault' ([System.Windows.Automation.ControlType]::Button) | Out-Null
  if (!$SkipClipboard) { Assert-Proof (!(Get-Clipboard)) 'suspend notification locked the view and cleared the clipboard' }
  Send-TestSignal 0x0218 18
  Unlock-FakeVault
  if (!$SkipClipboard) {
  Click-Control 'Open OpenAI prod proof'
  Assert-EmptyClipboard
  Click-Control 'Copy'
  Assert-Proof (Wait-Text 'Copied\.') 'copy completed before normal exit'
  $sequence = [AkmProofClipboard]::GetClipboardSequenceNumber()
  }
  $process.Refresh()
  [uint32]$affinity = 0
  Assert-Proof ([AkmProofClipboard]::GetWindowDisplayAffinity($process.MainWindowHandle, [ref]$affinity) -and $affinity -eq 0x11) 'the real window requests capture exclusion'
  $script:root.GetCurrentPattern([System.Windows.Automation.WindowPattern]::Pattern).Close()
  Assert-Proof ($process.WaitForExit(10000)) 'AKM exited normally'
  if (!$SkipClipboard) {
    Assert-Proof ([AkmProofClipboard]::GetClipboardSequenceNumber() -ne $sequence) 'normal exit cleared the owned clipboard copy'
    Assert-Proof (!(Get-Clipboard)) 'clipboard is empty after normal exit'
  } else { Write-Output 'SKIPPED: interactive clipboard copies/cleanup; the user clipboard was preserved.' }
  $success = $true
} catch {
  try { Save-ProofDiagnostics 'failed' } catch {}
  if ($script:root -and $process -and !$process.HasExited) {
    try {
      $names = $script:root.FindAll($TS::Descendants, [System.Windows.Automation.Condition]::TrueCondition) | Where-Object {
        $_.Current.Name -in @('File name:', '.env', 'Save', 'Cancel') -or $_.Current.AutomationId -in @('1001', '1148', '1152')
      } | ForEach-Object { '{0}: {1} (id={2}, class={3}, handle={4}, patterns={5})' -f $_.Current.ControlType.ProgrammaticName, $_.Current.Name, $_.Current.AutomationId, $_.Current.ClassName, $_.Current.NativeWindowHandle, (($_.GetSupportedPatterns() | ForEach-Object { $_.ProgrammaticName }) -join ',') }
      $names | Set-Content -LiteralPath (Join-Path $work 'fake-ui-names.txt') -Encoding UTF8
    } catch {}
  }
  throw
} finally {
  if ($process -and !$process.HasExited) {
    try { $process.CloseMainWindow() | Out-Null; if (!$process.WaitForExit(5000)) { $process.Kill() } } catch { $process.Kill() }
  }
  if (Test-Path -LiteralPath $settingsPath) {
    try {
      $current = [IO.File]::ReadAllText($settingsPath) | ConvertFrom-Json
      if ($ownedVaults -contains $current.vaultPath) {
        if ($null -eq $settingsBefore) { Remove-Item -LiteralPath $settingsPath }
        else { [IO.File]::WriteAllBytes($settingsPath, $settingsBefore) }
      } elseif ($null -eq $settingsBefore -or [Convert]::ToBase64String([IO.File]::ReadAllBytes($settingsPath)) -ne [Convert]::ToBase64String($settingsBefore)) {
        Write-Warning 'Settings changed outside this proof; they were not overwritten.'
      }
    } catch { Write-Warning 'Could not safely restore settings; inspect the proof settings path.' }
  }
  $settingsRestored = $false
  try {
    $settingsRestored = if ($null -eq $settingsBefore) { !(Test-Path -LiteralPath $settingsPath) }
      else { (Test-Path -LiteralPath $settingsPath -PathType Leaf) -and [Convert]::ToBase64String([IO.File]::ReadAllBytes($settingsPath)) -ceq [Convert]::ToBase64String($settingsBefore) }
  } catch { Write-Warning 'Settings restoration could not be verified.' }
  $recovery['settingsRestoredByteForByte'] = $settingsRestored
  $recovery['proofSucceeded'] = $success
  $recovery['finishedUtc'] = [DateTime]::UtcNow.ToString('o')
  $recovery | ConvertTo-Json | Set-Content -LiteralPath (Join-Path $work 'recovery.json') -Encoding UTF8
  if ($success -and $settingsRestored) { Remove-Item -LiteralPath $work -Recurse -Force }
  else { Write-Warning "Fake-vault evidence and settings preimage retained at $work" }
  if ($success -and !$settingsRestored) { throw 'Proof controls passed, but settings restoration did not: recovery evidence retained; do not rerun.' }
}
