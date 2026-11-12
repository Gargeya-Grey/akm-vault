# Binds an installed AKM program to the desktop build that SHA256SUMS.txt hashed (finding U4-R1-10).
#
# The Tauri bundler makes one change to the copy of the main program that it packs into the NSIS
# setup program: it replaces the first __TAURI_BUNDLE_TYPE_VAR_UNK with __TAURI_BUNDLE_TYPE_VAR_NSS,
# then restores the build file (tauri-bundler 2.10.4, src/bundle.rs, patch_binary; the pinned
# @tauri-apps/cli 2.12.4 requires that bundler version). So the installed program must be the build
# with that one change and no other. When the build has no such token, the bundler changes nothing,
# and the installed program must be the build itself.
#
# Reads files only. Used by tools/test_install.ps1; tested by tools/test_install_binding.ps1.

$script:Unmarked = '__TAURI_BUNDLE_TYPE_VAR_UNK'
$script:NsisMark = '__TAURI_BUNDLE_TYPE_VAR_NSS'
# Latin-1 maps each byte to one character, so a string search finds a byte offset.
$script:Bytes = [Text.Encoding]::GetEncoding(28591)

function Get-Sha256([byte[]]$data) {
    $sha = [Security.Cryptography.SHA256]::Create()
    try { return ([BitConverter]::ToString($sha.ComputeHash($data)) -replace '-', '').ToLowerInvariant() } finally { $sha.Dispose() }
}

function Compare-InstalledBuild {
    <#
    .SYNOPSIS
    Whether the installed program is the hashed build with only the bundler's NSIS mark.
    .OUTPUTS
    An object with Same (bool), Reason (empty when Same), BuiltSha256, InstalledSha256,
    ExpectedSha256 (the build with the mark) and MarkOffset (-1 when the build has no token).
    #>
    param(
        [Parameter(Mandatory = $true)][string]$Installed,
        [Parameter(Mandatory = $true)][string]$Built
    )
    $builtBytes = [IO.File]::ReadAllBytes($Built)
    $installedBytes = [IO.File]::ReadAllBytes($Installed)
    $offset = $script:Bytes.GetString($builtBytes).IndexOf($script:Unmarked, [StringComparison]::Ordinal)
    $expected = $builtBytes
    if ($offset -ge 0) {
        $expected = [byte[]]$builtBytes.Clone()
        $mark = $script:Bytes.GetBytes($script:NsisMark)
        [Array]::Copy($mark, 0, $expected, $offset, $mark.Length)
    }
    $result = [pscustomobject]@{
        Same = $false
        Reason = ''
        BuiltSha256 = Get-Sha256 $builtBytes
        InstalledSha256 = Get-Sha256 $installedBytes
        ExpectedSha256 = Get-Sha256 $expected
        MarkOffset = $offset
    }
    if ($installedBytes.Length -ne $builtBytes.Length) {
        $result.Reason = "the installed program has $($installedBytes.Length) bytes and the build has $($builtBytes.Length)"
    } elseif ($result.InstalledSha256 -eq $result.ExpectedSha256) {
        $result.Same = $true
    } elseif ($offset -ge 0 -and $result.InstalledSha256 -eq $result.BuiltSha256) {
        $result.Reason = "the installed program is the build without the bundler's NSIS mark at offset $offset, so this setup program did not come from the bundler step of this build"
    } else {
        $where = if ($offset -ge 0) { "with the bundler's NSIS mark at offset $offset" } else { 'itself (the build has no bundle-type token)' }
        $result.Reason = "the installed program (sha256 $($result.InstalledSha256)) is not the build $where (sha256 $($result.ExpectedSha256))"
    }
    return $result
}

function Assert-InstalledBuild {
    <# .SYNOPSIS Throws unless Compare-InstalledBuild finds the same program; else returns a one-line record. #>
    param(
        [Parameter(Mandatory = $true)][string]$Installed,
        [Parameter(Mandatory = $true)][string]$Built
    )
    $check = Compare-InstalledBuild -Installed $Installed -Built $Built
    if (-not $check.Same) { throw "the installed program is not the hashed build: $($check.Reason)" }
    return "installed sha256 $($check.InstalledSha256) = build sha256 $($check.BuiltSha256) with the NSIS mark at offset $($check.MarkOffset)"
}

Export-ModuleMember -Function Compare-InstalledBuild, Assert-InstalledBuild
