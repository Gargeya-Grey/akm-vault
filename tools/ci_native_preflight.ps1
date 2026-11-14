<#
.SYNOPSIS
Checks a disposable GitHub-hosted runner before a native AKM proof, and records what it found.

.DESCRIPTION
Used by .github/workflows/native-proof.yml. It runs only on a disposable runner
(tools/DisposableRunner.psm1). It:

1. checks every file in -Artifacts against its SHA256SUMS.txt, and refuses a file that the list
   does not name;
2. records the session facts that a real-window proof needs: the session ID, whether the input
   desktop is the interactive 'Default' desktop, the screen size and the clipboard format count;
3. records the WebView2 runtime version, and stops when it is missing and -NeedWebView2 is set.
   With -InstallWebView2 it installs the runtime when it is
   missing: it downloads Microsoft's bootstrapper, refuses it unless its Authenticode signature is
   valid and names Microsoft Corporation, runs it silently, and checks the version again.

It writes preflight.json, and copies SHA256SUMS.txt and build-info.txt, into -Evidence, a new folder
that must not exist. It prints one OK, INFO or STOP line per check and exits 1 after any STOP.
It deletes nothing.

.EXAMPLE
./tools/ci_native_preflight.ps1 -Artifacts $env:RUNNER_TEMP\build -Evidence $env:RUNNER_TEMP\evidence\gui -NeedDesktop -NeedEmptyClipboard -NeedWebView2 -InstallWebView2 -DisposableRunner
#>
param(
    [Parameter(Mandatory = $true)][string]$Artifacts,
    [Parameter(Mandatory = $true)][string]$Evidence,
    [switch]$NeedDesktop,
    [switch]$NeedEmptyClipboard,
    [switch]$NeedWebView2,
    [switch]$InstallWebView2,
    [switch]$DisposableRunner
)
$ErrorActionPreference = 'Stop'
$ProgressPreference = 'SilentlyContinue'
Import-Module (Join-Path $PSScriptRoot 'DisposableRunner.psm1') -Force
Assert-DisposableRunner -Tool 'tools/ci_native_preflight.ps1' -Confirmed:$DisposableRunner

if (Test-Path -LiteralPath $Evidence) { throw "REFUSED: the evidence folder $Evidence exists already. Nothing was changed." }
New-Item -ItemType Directory -Path $Evidence | Out-Null

$stop = $false
$facts = [ordered]@{ utc = [DateTime]::UtcNow.ToString('o') }
function Report([string]$level, [string]$text) {
    Write-Output "$level $text"
    if ($level -eq 'STOP') { $script:stop = $true }
}

# 1. The build under test.
$sums = Join-Path $Artifacts 'SHA256SUMS.txt'
if (-not (Test-Path -LiteralPath $sums -PathType Leaf)) { throw "STOP: $sums is missing." }
$listed = @{}
foreach ($line in Get-Content -LiteralPath $sums) {
    if ($line -notmatch '^([0-9a-f]{64})  (\S.*)$') { Report 'STOP' "SHA256SUMS.txt line is malformed: $line"; continue }
    $listed[$Matches[2]] = $Matches[1]
}
foreach ($name in $listed.Keys) {
    $path = Join-Path $Artifacts $name
    if (-not (Test-Path -LiteralPath $path -PathType Leaf)) { Report 'STOP' "$name is listed but missing"; continue }
    $actual = (Get-FileHash -LiteralPath $path -Algorithm SHA256).Hash.ToLowerInvariant()
    if ($actual -ne $listed[$name]) { Report 'STOP' "$name sha256=$actual, listed $($listed[$name])" }
    else { Report 'OK' "$name sha256=$actual" }
}
foreach ($file in Get-ChildItem -LiteralPath $Artifacts -File) {
    if ($file.Name -in @('SHA256SUMS.txt', 'build-info.txt')) { continue }
    if (-not $listed.ContainsKey($file.Name)) { Report 'STOP' "$($file.Name) is in the artifact but not in SHA256SUMS.txt" }
}
$facts.artifacts = $listed
foreach ($name in @('SHA256SUMS.txt', 'build-info.txt')) {
    $path = Join-Path $Artifacts $name
    if (Test-Path -LiteralPath $path) { Copy-Item -LiteralPath $path -Destination $Evidence }
}
$facts.commit = $env:GITHUB_SHA
$facts.runId = $env:GITHUB_RUN_ID
$facts.imageOs = $env:ImageOS
$facts.imageVersion = $env:ImageVersion
Report 'INFO' "commit=$env:GITHUB_SHA run=$env:GITHUB_RUN_ID image=$env:ImageOS $env:ImageVersion"

# 2. The session. These facts are not in the runner image documentation, so the proof measures them.
Add-Type -Namespace AkmPreflight -Name Native -MemberDefinition @'
[DllImport("kernel32.dll")] public static extern bool ProcessIdToSessionId(uint pid, out uint session);
[DllImport("kernel32.dll")] public static extern uint WTSGetActiveConsoleSessionId();
[DllImport("user32.dll", SetLastError=true)] public static extern IntPtr OpenInputDesktop(uint flags, bool inherit, uint access);
[DllImport("user32.dll")] public static extern bool CloseDesktop(IntPtr desktop);
[DllImport("user32.dll", CharSet=CharSet.Unicode, SetLastError=true)] public static extern bool GetUserObjectInformationW(IntPtr handle, int index, System.Text.StringBuilder name, uint length, out uint needed);
[DllImport("user32.dll")] public static extern int GetSystemMetrics(int index);
[DllImport("user32.dll")] public static extern int CountClipboardFormats();
'@
$session = [uint32]0
[void][AkmPreflight.Native]::ProcessIdToSessionId([uint32]$PID, [ref]$session)
$desktopName = 'unavailable'
$handle = [AkmPreflight.Native]::OpenInputDesktop(0, $false, 1)
if ($handle -ne [IntPtr]::Zero) {
    $name = New-Object System.Text.StringBuilder 256
    $needed = [uint32]0
    if ([AkmPreflight.Native]::GetUserObjectInformationW($handle, 2, $name, 512, [ref]$needed)) { $desktopName = $name.ToString() }
    [void][AkmPreflight.Native]::CloseDesktop($handle)
}
$facts.session = [ordered]@{
    id = $session
    activeConsole = [AkmPreflight.Native]::WTSGetActiveConsoleSessionId()
    userInteractive = [Environment]::UserInteractive
    inputDesktop = $desktopName
    screen = '{0}x{1}' -f [AkmPreflight.Native]::GetSystemMetrics(0), [AkmPreflight.Native]::GetSystemMetrics(1)
    clipboardFormats = [AkmPreflight.Native]::CountClipboardFormats()
}
$interactive = ($session -ne 0) -and ($desktopName -eq 'Default')
$sessionText = "session=$session inputDesktop=$desktopName screen=$($facts.session.screen) userInteractive=$([Environment]::UserInteractive)"
if ($NeedDesktop -and -not $interactive) { Report 'STOP' "no interactive desktop for UI Automation: $sessionText" }
else { Report 'OK' $sessionText }
if ($NeedEmptyClipboard -and $facts.session.clipboardFormats -ne 0) { Report 'STOP' "the clipboard holds $($facts.session.clipboardFormats) formats; the proofs need an empty clipboard" }
else { Report 'OK' "clipboard formats=$($facts.session.clipboardFormats)" }

# 3. The WebView2 runtime.
function Get-WebView2Version {
    $guid = '{F3017226-FE2A-4295-8BDF-00C3A9A7E4C5}'
    foreach ($key in @("HKLM:\SOFTWARE\WOW6432Node\Microsoft\EdgeUpdate\Clients\$guid", "HKLM:\SOFTWARE\Microsoft\EdgeUpdate\Clients\$guid", "HKCU:\SOFTWARE\Microsoft\EdgeUpdate\Clients\$guid")) {
        $value = (Get-ItemProperty -LiteralPath $key -Name pv -ErrorAction SilentlyContinue).pv
        if ($value -and $value -ne '0.0.0.0') { return $value }
    }
    return $null
}
$webview = Get-WebView2Version
$facts.webView2Before = $webview
if (-not $webview -and $InstallWebView2) {
    $folder = Join-Path $env:RUNNER_TEMP ('akm-webview2-' + [Guid]::NewGuid().ToString('N'))
    New-Item -ItemType Directory -Path $folder | Out-Null
    $setup = Join-Path $folder 'MicrosoftEdgeWebview2Setup.exe'
    [Net.ServicePointManager]::SecurityProtocol = [Net.SecurityProtocolType]::Tls12
    Invoke-WebRequest -Uri 'https://go.microsoft.com/fwlink/p/?LinkId=2124703' -OutFile $setup -UseBasicParsing
    $signature = Get-AuthenticodeSignature -LiteralPath $setup
    $facts.webView2Bootstrapper = [ordered]@{
        sha256 = (Get-FileHash -LiteralPath $setup -Algorithm SHA256).Hash.ToLowerInvariant()
        signature = [string]$signature.Status
        signer = [string]$signature.SignerCertificate.Subject
    }
    if ($signature.Status -ne 'Valid' -or $signature.SignerCertificate.Subject -notmatch 'O=Microsoft Corporation') {
        Report 'STOP' "the WebView2 bootstrapper is not validly signed by Microsoft: $($signature.Status) $($signature.SignerCertificate.Subject)"
    } else {
        $run = Start-Process -FilePath $setup -ArgumentList '/silent', '/install' -Wait -PassThru
        $facts.webView2BootstrapperExit = $run.ExitCode
        $webview = Get-WebView2Version
    }
}
$facts.webView2 = $webview
if ($webview) { Report 'OK' "WebView2 runtime $webview" }
elseif ($NeedWebView2) { Report 'STOP' 'no WebView2 runtime; the desktop app cannot start' }
else { Report 'INFO' 'no WebView2 runtime' }

$facts.result = if ($stop) { 'STOP' } else { 'OK' }
$facts | ConvertTo-Json -Depth 5 | Set-Content -LiteralPath (Join-Path $Evidence 'preflight.json') -Encoding UTF8
if ($stop) { exit 1 }
