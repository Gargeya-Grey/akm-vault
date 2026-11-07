<#
.SYNOPSIS
Tests that tools/fetch_keepassxc.ps1 never deletes content that it did not create (review 1
finding 8). Uses a fake zip and fake digest in a new temporary folder; no network.

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
function Fetch([string]$dest, [string]$digest, [switch]$Force) {
    $argList = @('-NoProfile', '-ExecutionPolicy', 'Bypass', '-File', $tool, '-Destination', $dest, '-LocalZip', $zip, '-LocalDigest', $digest)
    if ($Force) { $argList += '-Force' }
    $ErrorActionPreference = 'Continue'
    $output = & powershell @argList 2>&1 | Out-String
    return [pscustomobject]@{ Exit = $LASTEXITCODE; Output = $output }
}

New-Item -ItemType Directory -Path $root | Out-Null
try {
    $src = Join-Path $root 'src\KeePassXC-2.8.0-x64'
    New-Item -ItemType Directory -Path $src | Out-Null
    Set-Content -LiteralPath (Join-Path $src 'keepassxc-cli.exe') -Value 'fake cli'
    $zip = Join-Path $root 'KeePassXC-2.8.0-x64.zip'
    Compress-Archive -Path (Join-Path $root 'src\*') -DestinationPath $zip
    $good = Join-Path $root 'good.DIGEST'
    Set-Content -LiteralPath $good -Value ((Get-FileHash -LiteralPath $zip -Algorithm SHA256).Hash.ToLowerInvariant() + '  KeePassXC-2.8.0-x64.zip')
    $bad = Join-Path $root 'bad.DIGEST'
    Set-Content -LiteralPath $bad -Value (('0' * 64) + '  KeePassXC-2.8.0-x64.zip')

    $shared = Join-Path $root 'archive-parent'
    New-Item -ItemType Directory -Path $shared | Out-Null
    Set-Content -LiteralPath (Join-Path $shared 'evidence.txt') -Value 'unrelated sentinel'
    $r = Fetch $shared $good -Force
    Check ($r.Exit -ne 0 -and (Test-Path -LiteralPath (Join-Path $shared 'evidence.txt'))) 'a folder with unrelated content is refused with -Force and its content survives'

    $owned = Join-Path $root 'owned'
    $r = Fetch $owned $good
    Check ($r.Exit -eq 0 -and (Test-Path -LiteralPath (Join-Path $owned '.akm-fetch-keepassxc')) -and (Test-Path -LiteralPath (Join-Path $owned 'KeePassXC-2.8.0-x64\keepassxc-cli.exe'))) 'a missing folder is created, marked and filled'

    $r = Fetch $owned $good
    Check ($r.Exit -ne 0) 'an owned folder is not replaced without -Force'

    $before = (Get-FileHash -LiteralPath (Join-Path $owned 'KeePassXC-2.8.0-x64\keepassxc-cli.exe')).Hash
    $r = Fetch $owned $bad -Force
    Check ($r.Exit -ne 0 -and $r.Output -match 'not changed' -and (Get-FileHash -LiteralPath (Join-Path $owned 'KeePassXC-2.8.0-x64\keepassxc-cli.exe')).Hash -eq $before) 'a digest mismatch leaves the earlier fetch in place'

    $r = Fetch $owned $good -Force
    Check ($r.Exit -eq 0 -and (Test-Path -LiteralPath (Join-Path $owned 'KeePassXC-2.8.0-x64\keepassxc-cli.exe'))) 'an owned folder is replaced with -Force after the digest check'

    Set-Content -LiteralPath (Join-Path $owned 'notes.txt') -Value 'added by a person'
    $r = Fetch $owned $good -Force
    Check ($r.Exit -ne 0 -and (Test-Path -LiteralPath (Join-Path $owned 'notes.txt'))) 'an owned folder that gained a foreign file is refused and the file survives'

    $target = Join-Path $root 'junction-target'
    New-Item -ItemType Directory -Path $target | Out-Null
    Set-Content -LiteralPath (Join-Path $target 'keep.txt') -Value 'behind a junction'
    $junction = Join-Path $root 'junction'
    cmd /c mklink /J "$junction" "$target" | Out-Null
    $r = Fetch $junction $good -Force
    Check ($r.Exit -ne 0 -and (Test-Path -LiteralPath (Join-Path $target 'keep.txt'))) 'a junction destination is refused and its target survives'

    $drive = [IO.Path]::GetPathRoot($root)
    $r = Fetch $drive $good -Force
    Check ($r.Exit -ne 0 -and $r.Output -match 'drive root') "a drive root ($drive) is refused"
} finally {
    if (Test-Path -LiteralPath (Join-Path $root 'junction')) { cmd /c rmdir "$(Join-Path $root 'junction')" | Out-Null }
    Remove-Item -LiteralPath $root -Recurse -Force -ErrorAction SilentlyContinue
}
if ($failures) { Write-Output "$failures case(s) failed"; exit 1 }
Write-Output 'all cases passed'
