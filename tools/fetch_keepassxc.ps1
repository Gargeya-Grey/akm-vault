<#
.SYNOPSIS
Downloads portable KeePassXC (Windows x64 zip) for tools/verify_keepassxc.py and checks it.

.DESCRIPTION
Downloads the official zip and its .DIGEST file from the KeePassXC GitHub release.
Compares the SHA-256 of the zip with the digest and stops on a mismatch.
Extracts only after the check passes. The destination must be outside the repository.
When the requested version has no x64 zip, it uses the newest published 2.7.x or 2.8.x release
that has one, and says so.

.EXAMPLE
./tools/fetch_keepassxc.ps1
python tools/verify_keepassxc.py --akm target/release/akm.exe --keepassxc W:/AKM-archive/tools/keepassxc-2.8.0/<folder>/keepassxc-cli.exe
#>
[CmdletBinding()]
param(
    [string]$Version = '2.8.0',
    [string]$Destination = 'W:\AKM-archive\tools\keepassxc-2.8.0\',
    [switch]$Force
)

$ErrorActionPreference = 'Stop'
$ProgressPreference = 'SilentlyContinue'
[Net.ServicePointManager]::SecurityProtocol = [Net.SecurityProtocolType]::Tls12

$repoRoot = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..')).TrimEnd('\') + '\'
$destFull = [IO.Path]::GetFullPath($Destination).TrimEnd('\') + '\'
if ($destFull.StartsWith($repoRoot, [StringComparison]::OrdinalIgnoreCase)) {
    throw "Destination $destFull is inside the repository $repoRoot. Choose a folder outside it."
}

$base = 'https://github.com/keepassxreboot/keepassxc/releases/download'
$headers = @{ 'User-Agent' = 'akm-fetch-keepassxc' }

function Test-Url([string]$Url) {
    try {
        Invoke-WebRequest -Uri $Url -Method Head -UseBasicParsing -Headers $headers | Out-Null
        return $true
    } catch {
        return $false
    }
}

$asset = "KeePassXC-$Version-x64.zip"
if (-not (Test-Url "$base/$Version/$asset.DIGEST")) {
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
    Write-Host "Using KeePassXC $Version instead."
}

if ((Test-Path -LiteralPath $destFull) -and (Get-ChildItem -LiteralPath $destFull -Force | Select-Object -First 1)) {
    if (-not $Force) { throw "Destination $destFull is not empty. Use -Force to replace it." }
    Remove-Item -LiteralPath $destFull -Recurse -Force
}

$work = Join-Path ([IO.Path]::GetTempPath()) ("akm-fetch-keepassxc-" + [Guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Path $work | Out-Null
try {
    $zipUrl = "$base/$Version/$asset"
    $zipPath = Join-Path $work $asset
    Write-Host "Downloading $zipUrl"
    Invoke-WebRequest -Uri $zipUrl -OutFile $zipPath -UseBasicParsing -Headers $headers
    $digestText = (Invoke-WebRequest -Uri "$zipUrl.DIGEST" -UseBasicParsing -Headers $headers).Content
    if ($digestText -is [byte[]]) { $digestText = [Text.Encoding]::UTF8.GetString($digestText) }

    $match = [regex]::Match($digestText, '(?im)^\s*([0-9a-f]{64})\b')
    if (-not $match.Success) { throw "The DIGEST file has no SHA-256 value: $digestText" }
    $expected = $match.Groups[1].Value.ToLowerInvariant()
    $actual = (Get-FileHash -LiteralPath $zipPath -Algorithm SHA256).Hash.ToLowerInvariant()
    if ($actual -ne $expected) {
        throw "SHA-256 mismatch for $asset. Expected $expected from the DIGEST file, got $actual. Nothing was extracted."
    }
    Write-Host "SHA-256 OK: $actual"

    New-Item -ItemType Directory -Path $destFull -Force | Out-Null
    Expand-Archive -LiteralPath $zipPath -DestinationPath $destFull -Force
} finally {
    Remove-Item -LiteralPath $work -Recurse -Force -ErrorAction SilentlyContinue
}

$cli = Get-ChildItem -LiteralPath $destFull -Recurse -Filter 'keepassxc-cli.exe' | Select-Object -First 1
if ($null -eq $cli) { throw "keepassxc-cli.exe not found under $destFull" }
Write-Host "KeePassXC $Version"
Write-Host "URL      $zipUrl"
Write-Host "SHA256   $actual"
Write-Host "CLI      $($cli.FullName)"
