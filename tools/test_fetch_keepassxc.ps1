<#
.SYNOPSIS
Tests that tools/fetch_keepassxc.ps1 never deletes or replaces anything (review 1 finding 8,
review 2 finding P2R2-01). Uses a fake zip and fake digest in new temporary folders; no network.

.EXAMPLE
powershell -NoProfile -ExecutionPolicy Bypass -File tools/test_fetch_keepassxc.ps1
#>
$ErrorActionPreference = 'Stop'
$tool = Join-Path $PSScriptRoot 'fetch_keepassxc.ps1'
$root = Join-Path ([IO.Path]::GetTempPath()) ("akm-test-fetch-" + [Guid]::NewGuid().ToString('N'))
$failures = 0

function Check([bool]$ok, [string]$text) {
    if ($ok) { Write-Output "ok   $text" } else { Write-Output "FAIL $text"; $script:failures++ }
}
function Fetch([string]$dest, [string]$digest) {
    $argList = @('-NoProfile', '-ExecutionPolicy', 'Bypass', '-File', $tool, '-Destination', $dest, '-LocalZip', $zip, '-LocalDigest', $digest)
    $ErrorActionPreference = 'Continue'
    $output = & powershell @argList 2>&1 | Out-String
    return [pscustomobject]@{ Exit = $LASTEXITCODE; Output = $output }
}
# Every file under a folder, with its hash: a fetch may add a folder, and must change nothing else.
function Snapshot([string]$path) {
    Get-ChildItem -LiteralPath $path -Recurse -Force -File | ForEach-Object {
        "$($_.FullName.Substring($path.Length)) $((Get-FileHash -LiteralPath $_.FullName).Hash)"
    } | Sort-Object
}
function Same($a, $b) { -not (Compare-Object @($a) @($b)) }

New-Item -ItemType Directory -Path $root | Out-Null
try {
    # Structural guard (class F): the only Remove-Item in the tool removes its own temp folder.
    $removals = Select-String -LiteralPath $tool -Pattern 'Remove-Item|\.Delete\(|rmdir|\brd\b|del ' | Where-Object { $_.Line -notmatch '^\s*#' }
    Check ((@($removals).Count -eq 1) -and ($removals[0].Line -match 'Remove-Item -LiteralPath \$work ')) 'the tool has one removal, and it removes only its own temporary folder'
    Check (-not (Select-String -LiteralPath $tool -Pattern '\[switch\]\$Force' -Quiet)) 'the tool has no -Force switch'

    $src = Join-Path $root 'src\KeePassXC-2.8.0-x64'
    New-Item -ItemType Directory -Path $src | Out-Null
    Set-Content -LiteralPath (Join-Path $src 'keepassxc-cli.exe') -Value 'fake cli'
    $zip = Join-Path $root 'KeePassXC-2.8.0-x64.zip'
    Compress-Archive -Path (Join-Path $root 'src\*') -DestinationPath $zip
    $sha = (Get-FileHash -LiteralPath $zip -Algorithm SHA256).Hash.ToLowerInvariant()
    $good = Join-Path $root 'good.DIGEST'
    Set-Content -LiteralPath $good -Value ($sha + '  KeePassXC-2.8.0-x64.zip')
    $bad = Join-Path $root 'bad.DIGEST'
    Set-Content -LiteralPath $bad -Value (('0' * 64) + '  KeePassXC-2.8.0-x64.zip')
    $folder = 'KeePassXC-2.8.0-x64-' + $sha.Substring(0, 12)

    $dest = Join-Path $root 'tools'
    $r = Fetch $dest $good
    $cli = Join-Path $dest "$folder\KeePassXC-2.8.0-x64\keepassxc-cli.exe"
    Check ($r.Exit -eq 0 -and (Test-Path -LiteralPath $cli) -and (Test-Path -LiteralPath (Join-Path $dest "$folder\.akm-fetch-keepassxc"))) 'a missing destination gets one new marked folder named by version and digest'

    # The reviewer's case: files added inside a fetched folder, at every depth.
    Set-Content -LiteralPath (Join-Path $dest "$folder\KeePassXC-2.8.0-x64\user-evidence.txt") -Value 'nested sentinel'
    New-Item -ItemType Directory -Path (Join-Path $dest "$folder\notes\deeper") | Out-Null
    Set-Content -LiteralPath (Join-Path $dest "$folder\notes\deeper\keep.txt") -Value 'deeper sentinel'
    Set-Content -LiteralPath (Join-Path $dest 'evidence.txt') -Value 'top-level sentinel'
    $before = Snapshot $dest
    $r = Fetch $dest $good
    Check ($r.Exit -eq 0 -and $r.Output -match 'nothing was changed' -and (Same $before (Snapshot $dest))) 'a second fetch of the same release changes nothing, nested files included'

    $r = Fetch $dest $bad
    Check ($r.Exit -ne 0 -and $r.Output -match 'Nothing was changed' -and (Same $before (Snapshot $dest))) 'a digest mismatch changes nothing'

    $foreign = Join-Path $root 'foreign'
    New-Item -ItemType Directory -Path (Join-Path $foreign $folder) | Out-Null
    Set-Content -LiteralPath (Join-Path $foreign "$folder\mine.txt") -Value 'a folder of the same name, not made by the tool'
    $beforeForeign = Snapshot $foreign
    $r = Fetch $foreign $good
    Check ($r.Exit -ne 0 -and (Same $beforeForeign (Snapshot $foreign))) 'an unmarked folder of the same name is refused and left as it was'

    $target = Join-Path $root 'junction-target'
    New-Item -ItemType Directory -Path $target | Out-Null
    Set-Content -LiteralPath (Join-Path $target 'keep.txt') -Value 'behind a junction'
    $junction = Join-Path $root 'junction'
    cmd /c mklink /J "$junction" "$target" | Out-Null
    $r = Fetch $junction $good
    Check ($r.Exit -ne 0 -and @(Get-ChildItem -LiteralPath $target -Force).Count -eq 1) 'a junction destination is refused and its target is unchanged'
    $r = Fetch (Join-Path $junction 'below') $good
    Check ($r.Exit -ne 0 -and @(Get-ChildItem -LiteralPath $target -Force).Count -eq 1) 'a destination below a junction is refused and the junction target is unchanged'

    # Both the temp drive and the drive of the current folder: CI found that "D:" without its
    # backslash resolved to the current folder on D:, so the old check missed that drive's root.
    foreach ($drive in @([IO.Path]::GetPathRoot($root), [IO.Path]::GetPathRoot((Get-Location).Path)) | Select-Object -Unique) {
        $r = Fetch $drive $good
        Check ($r.Exit -ne 0 -and $r.Output -match 'drive root') "a drive root ($drive) is refused"
    }
} finally {
    if (Test-Path -LiteralPath (Join-Path $root 'junction')) { cmd /c rmdir "$(Join-Path $root 'junction')" | Out-Null }
    Remove-Item -LiteralPath $root -Recurse -Force -ErrorAction SilentlyContinue
}
if ($failures) { Write-Output "$failures case(s) failed"; exit 1 }
Write-Output 'all cases passed'
