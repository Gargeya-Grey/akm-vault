<#
.SYNOPSIS
Downloads portable KeePassXC (Windows x64 zip) for tools/verify_keepassxc.py and checks it.

.DESCRIPTION
Downloads the official zip and its .DIGEST file from the KeePassXC GitHub release, compares the
SHA-256 of the zip with the digest, and extracts it in a staging folder. Only after that check
does it touch the destination. The destination must be outside the repository, must not be a
drive root or a reparse point, and must be missing, empty, or a folder that this tool created
(it holds the marker file .akm-fetch-keepassxc and nothing but KeePassXC-*-x64 folders). With
-Force, it replaces only such a folder. Any other content is refused and left untouched.
When the requested version has no x64 zip, it uses the newest published 2.7.x or 2.8.x release
that has one, and says so. -LocalZip and -LocalDigest use local files instead of the download
(tools/test_fetch_keepassxc.ps1 uses them).

.EXAMPLE
./tools/fetch_keepassxc.ps1
python tools/verify_keepassxc.py --akm target/release/akm.exe --keepassxc W:/AKM-archive/tools/keepassxc-2.8.0/<folder>/keepassxc-cli.exe
#>
[CmdletBinding()]
param(
    [string]$Version = '2.8.0',
    [string]$Destination = 'W:\AKM-archive\tools\keepassxc-2.8.0\',
    [switch]$Force,
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
if ($null -eq [IO.Directory]::GetParent($destFull.TrimEnd('\'))) {
    throw "Destination $destFull is a drive root. Choose a folder of its own."
}

function Assert-Replaceable([string]$Path) {
    if (-not (Test-Path -LiteralPath $Path)) { return }
    $item = Get-Item -LiteralPath $Path -Force
    if (-not $item.PSIsContainer) { throw "Destination $Path is a file, not a folder. Nothing was changed." }
    if ($item.Attributes -band [IO.FileAttributes]::ReparsePoint) { throw "Destination $Path is a reparse point (a junction or link). Nothing was changed." }
    $children = @(Get-ChildItem -LiteralPath $Path -Force)
    if ($children.Count -eq 0) { return }
    $owned = $children | Where-Object { $_.Name -eq $Marker }
    $foreign = @($children | Where-Object { $_.Name -ne $Marker -and -not ($_.PSIsContainer -and $_.Name -like 'KeePassXC-*-x64') })
    if (-not $owned -or $foreign.Count) {
        throw "Destination $Path holds content that this tool did not create ($(@($foreign.Name) -join ', ')). Nothing was changed. Choose an empty folder."
    }
    if (-not $Force) { throw "Destination $Path already holds a KeePassXC that this tool fetched. Use -Force to replace it." }
}

Assert-Replaceable $destFull

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
        throw "SHA-256 mismatch for $asset. Expected $expected from the DIGEST file, got $actual. The destination was not changed."
    }
    Write-Host "SHA-256 OK: $actual"

    $staged = Join-Path $work 'extract'
    Expand-Archive -LiteralPath $zipPath -DestinationPath $staged
    if (-not (Get-ChildItem -LiteralPath $staged -Recurse -Filter 'keepassxc-cli.exe' | Select-Object -First 1)) {
        throw "keepassxc-cli.exe is not in $asset. The destination was not changed."
    }

    Assert-Replaceable $destFull
    if (Test-Path -LiteralPath $destFull) {
        Get-ChildItem -LiteralPath $destFull -Force | Where-Object { $_.Name -ne $Marker } | Remove-Item -Recurse -Force
    } else {
        New-Item -ItemType Directory -Path $destFull | Out-Null
    }
    Set-Content -LiteralPath (Join-Path $destFull $Marker) -Value "Created by tools/fetch_keepassxc.ps1 for KeePassXC $Version ($actual)." -Encoding UTF8
    Get-ChildItem -LiteralPath $staged -Force | Move-Item -Destination $destFull
} finally {
    Remove-Item -LiteralPath $work -Recurse -Force -ErrorAction SilentlyContinue
}

$cli = Get-ChildItem -LiteralPath $destFull -Recurse -Filter 'keepassxc-cli.exe' | Select-Object -First 1
Write-Host "KeePassXC $Version"
Write-Host "URL      $zipUrl"
Write-Host "SHA256   $actual"
Write-Host "CLI      $($cli.FullName)"
