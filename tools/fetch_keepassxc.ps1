<#
.SYNOPSIS
Downloads portable KeePassXC (Windows x64 zip) for tools/verify_keepassxc.py and checks it.

.DESCRIPTION
Downloads the official zip and its .DIGEST file from the KeePassXC GitHub release, compares the
SHA-256 of the zip with the digest, and extracts it in a temporary folder. Only then does it add
one new folder, KeePassXC-<version>-<first 12 digest characters>, under the destination.

The tool never deletes or replaces anything outside its own temporary folder (owner decision,
2026-11-07, review 2 finding P2R2-01). A second fetch of the same release finds its folder and
changes nothing. An existing folder of that name that the tool did not mark is refused. Remove
old folders by hand. The destination must be outside the repository, must not be a drive root,
and neither it nor any folder above it may be a junction or a link.

When the requested version has no x64 zip, it uses the newest published 2.7.x or 2.8.x release
that has one, and says so. -LocalZip and -LocalDigest use local files instead of the download
(tools/test_fetch_keepassxc.ps1 uses them).

.EXAMPLE
./tools/fetch_keepassxc.ps1
#>
[CmdletBinding()]
param(
    [string]$Version = '2.8.0',
    [string]$Destination = 'W:\AKM-archive\tools\keepassxc\',
    [string]$LocalZip,
    [string]$LocalDigest
)

$ErrorActionPreference = 'Stop'
$ProgressPreference = 'SilentlyContinue'
$Marker = '.akm-fetch-keepassxc'

$repoRoot = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..')).TrimEnd('\') + '\'
$destFull = [IO.Path]::GetFullPath($Destination).TrimEnd('\') + '\'
if ($destFull.StartsWith($repoRoot, [StringComparison]::OrdinalIgnoreCase)) {
    throw "Destination $destFull is inside the repository $repoRoot. Choose a folder outside it."
}
# Compare with the root itself: "D:" without its backslash means the current folder on D:.
if ([IO.Path]::GetPathRoot($destFull) -eq $destFull) {
    throw "Destination $destFull is a drive root. Choose a folder of its own."
}

# The destination and every existing folder above it: a junction or link there could send the
# new folder somewhere else.
function Assert-NoReparseAbove([string]$Path) {
    $current = [IO.DirectoryInfo]::new($Path.TrimEnd('\'))
    while ($null -ne $current) {
        if ($current.Exists -and ($current.Attributes -band [IO.FileAttributes]::ReparsePoint)) {
            throw "$($current.FullName) is a junction or link. Choose a destination without one. Nothing was changed."
        }
        $current = $current.Parent
    }
}
Assert-NoReparseAbove $destFull
if ((Test-Path -LiteralPath $destFull) -and -not (Get-Item -LiteralPath $destFull -Force).PSIsContainer) {
    throw "Destination $destFull is a file, not a folder. Nothing was changed."
}

$base = 'https://github.com/keepassxreboot/keepassxc/releases/download'
$headers = @{ 'User-Agent' = 'akm-fetch-keepassxc' }
$asset = "KeePassXC-$Version-x64.zip"
$zipUrl = "$base/$Version/$asset"

$work = Join-Path ([IO.Path]::GetTempPath()) ("akm-fetch-keepassxc-" + [Guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Path $work | Out-Null
try {
    $zipPath = Join-Path $work $asset
    if ($LocalZip) {
        Copy-Item -LiteralPath $LocalZip -Destination $zipPath
        $digestText = Get-Content -LiteralPath $LocalDigest -Raw
        $zipUrl = "local file $LocalZip"
    } else {
        [Net.ServicePointManager]::SecurityProtocol = [Net.SecurityProtocolType]::Tls12
        try { Invoke-WebRequest -Uri "$zipUrl.DIGEST" -Method Head -UseBasicParsing -Headers $headers | Out-Null }
        catch {
            Write-Host "Version $Version has no $asset. Looking for the newest published 2.7.x or 2.8.x release."
            $releases = Invoke-RestMethod -Uri 'https://api.github.com/repos/keepassxreboot/keepassxc/releases?per_page=50' -Headers $headers
            $candidate = $releases |
                Where-Object { -not $_.prerelease -and -not $_.draft -and $_.tag_name -match '^2\.(7|8)\.\d+$' } |
                Where-Object { $_.assets.name -contains "KeePassXC-$($_.tag_name)-x64.zip" -and $_.assets.name -contains "KeePassXC-$($_.tag_name)-x64.zip.DIGEST" } |
                Sort-Object { [version]$_.tag_name } -Descending |
                Select-Object -First 1
            if ($null -eq $candidate) { throw 'No published 2.7.x or 2.8.x release has an x64 zip with a DIGEST file.' }
            $Version = $candidate.tag_name
            $asset = "KeePassXC-$Version-x64.zip"
            $zipUrl = "$base/$Version/$asset"
            $zipPath = Join-Path $work $asset
            Write-Host "Using KeePassXC $Version instead."
        }
        Write-Host "Downloading $zipUrl"
        Invoke-WebRequest -Uri $zipUrl -OutFile $zipPath -UseBasicParsing -Headers $headers
        $digestText = (Invoke-WebRequest -Uri "$zipUrl.DIGEST" -UseBasicParsing -Headers $headers).Content
        if ($digestText -is [byte[]]) { $digestText = [Text.Encoding]::UTF8.GetString($digestText) }
    }

    $match = [regex]::Match($digestText, '(?im)^\s*([0-9a-f]{64})\b')
    if (-not $match.Success) { throw "The DIGEST file has no SHA-256 value: $digestText" }
    $expected = $match.Groups[1].Value.ToLowerInvariant()
    $actual = (Get-FileHash -LiteralPath $zipPath -Algorithm SHA256).Hash.ToLowerInvariant()
    if ($actual -ne $expected) {
        throw "SHA-256 mismatch for $asset. Expected $expected from the DIGEST file, got $actual. Nothing was changed."
    }
    Write-Host "SHA-256 OK: $actual"

    $target = Join-Path $destFull ("KeePassXC-$Version-x64-" + $actual.Substring(0, 12))
    $markerText = "Created by tools/fetch_keepassxc.ps1 for KeePassXC $Version ($actual)."
    if (Test-Path -LiteralPath $target) {
        $markerPath = Join-Path $target $Marker
        $owned = (Test-Path -LiteralPath $markerPath) -and ((Get-Content -LiteralPath $markerPath -Raw).Trim() -eq $markerText)
        if (-not $owned) { throw "$target exists and this tool did not create it. Nothing was changed." }
        Write-Host "Already fetched; nothing was changed."
    } else {
        $staged = Join-Path $work 'extract'
        Expand-Archive -LiteralPath $zipPath -DestinationPath $staged
        if (-not (Get-ChildItem -LiteralPath $staged -Recurse -Filter 'keepassxc-cli.exe' | Select-Object -First 1)) {
            throw "keepassxc-cli.exe is not in $asset. Nothing was changed."
        }
        New-Item -ItemType Directory -Path $destFull -Force | Out-Null
        Assert-NoReparseAbove $destFull
        # A copy, because the temporary folder can be on another drive. The marker comes last, so
        # a copy that stops halfway leaves an unmarked folder, which the next run refuses.
        Copy-Item -LiteralPath $staged -Destination $target -Recurse
        Set-Content -LiteralPath (Join-Path $target $Marker) -Value $markerText -Encoding UTF8
    }
} finally {
    # The only removal in this tool: its own temporary folder, made above with a new GUID.
    Remove-Item -LiteralPath $work -Recurse -Force -ErrorAction SilentlyContinue
}

$cli = Get-ChildItem -LiteralPath $target -Recurse -Filter 'keepassxc-cli.exe' | Select-Object -First 1
Write-Host "KeePassXC $Version"
Write-Host "URL      $zipUrl"
Write-Host "SHA256   $actual"
Write-Host "CLI      $($cli.FullName)"
