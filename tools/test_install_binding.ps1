# The install test binds the installed program to the hashed desktop build (finding U4-R1-10).
# Two parts, and neither installs anything:
# - Compare-InstalledBuild (tools/InstallBinding.psm1) on fake program files in a new temporary folder:
#   the build with the bundler's NSIS mark matches; a changed byte, a missing or wrong mark, a mark on
#   the second token and a different length do not.
# - tools/test_install.ps1 takes the build with a mandatory -DesktopExe and checks each install with
#   Assert-InstalledBuild, and the native-proof workflow passes the build that its preflight hashed.
# Usage: powershell -NoProfile -ExecutionPolicy Bypass -File tools/test_install_binding.ps1
$ErrorActionPreference = 'Stop'
Import-Module (Join-Path $PSScriptRoot 'InstallBinding.psm1') -Force
$failures = 0
function Report([bool]$ok, [string]$name, [string]$detail) {
    if ($ok) { Write-Output "ok   $name" } else { Write-Output "FAIL $name ($detail)"; $script:failures++ }
}

$latin = [Text.Encoding]::GetEncoding(28591)
$every = -join [char[]](0..255)
# The two bytes C3 A9 are one UTF-8 character, so a search that decodes UTF-8 finds a wrong offset.
$head = 'MZ fake program head ' + ($every * 20) + [char]0xC3 + [char]0xA9
$middle = ' fake middle ' + ($every * 3)
$tail = ' fake program tail ' + ($every * 10)
$unk = '__TAURI_BUNDLE_TYPE_VAR_UNK'
$nss = '__TAURI_BUNDLE_TYPE_VAR_NSS'
$msi = '__TAURI_BUNDLE_TYPE_VAR_MSI'
$built = $head + $unk + $tail
$changed = $tail.Substring(0, 40) + [char](([int]$tail[40] + 1) % 256) + $tail.Substring(41)

# (name, build text, installed text, same?, a part of the reason)
$cases = @(
    @('the build with the NSIS mark in place of its token matches', $built, ($head + $nss + $tail), $true, ''),
    @('one other changed byte fails', $built, ($head + $nss + $changed), $false, "is not the build with the bundler's NSIS mark at offset $($head.Length)"),
    @('the build itself, without the NSIS mark, fails', $built, $built, $false, "without the bundler's NSIS mark at offset $($head.Length)"),
    @('an MSI mark in place of the NSIS mark fails', $built, ($head + $msi + $tail), $false, 'is not the build with'),
    @('with two tokens, the mark on the first one matches', ($head + $unk + $middle + $unk + $tail), ($head + $nss + $middle + $unk + $tail), $true, ''),
    @('with two tokens, a mark on the second one fails', ($head + $unk + $middle + $unk + $tail), ($head + $unk + $middle + $nss + $tail), $false, 'is not the build with'),
    @('a longer installed program fails', $built, ($head + $nss + $tail + 'z'), $false, 'bytes and the build has'),
    @('a build without a token matches only itself', ($head + $tail), ($head + $tail), $true, ''),
    @('a build without a token: one changed byte fails', ($head + $tail), ($head + $changed), $false, 'is not the build itself (the build has no bundle-type token)')
)

$work = Join-Path ([IO.Path]::GetTempPath()) ('akm-install-binding-test-' + [Guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Path $work | Out-Null
try {
    $index = 0
    foreach ($case in $cases) {
        $name, $buildText, $installedText, $same, $reason = $case
        $buildFile = Join-Path $work "build-$index.exe"
        $installedFile = Join-Path $work "installed-$index.exe"
        [IO.File]::WriteAllBytes($buildFile, $latin.GetBytes($buildText))
        [IO.File]::WriteAllBytes($installedFile, $latin.GetBytes($installedText))
        $check = Compare-InstalledBuild -Installed $installedFile -Built $buildFile
        $ok = $check.Same -eq $same -and ($same -or $check.Reason.Contains($reason)) -and (-not $same -or $check.Reason -eq '')
        Report $ok $name "same=$($check.Same); reason: $($check.Reason)"
        $index++
    }

    $buildFile = Join-Path $work 'build-0.exe'
    $check = Compare-InstalledBuild -Installed (Join-Path $work 'installed-0.exe') -Built $buildFile
    $fileHash = (Get-FileHash -LiteralPath $buildFile -Algorithm SHA256).Hash.ToLowerInvariant()
    Report ($check.BuiltSha256 -eq $fileHash -and $check.MarkOffset -eq $head.Length) 'the result names the hash of the build file and the offset of its token' "built=$($check.BuiltSha256) file=$fileHash offset=$($check.MarkOffset)"

    $message = ''
    try { Assert-InstalledBuild -Installed (Join-Path $work 'installed-1.exe') -Built (Join-Path $work 'build-1.exe') | Out-Null } catch { $message = $_.Exception.Message }
    Report ($message -like 'the installed program is not the hashed build: *') 'Assert-InstalledBuild throws on a mismatch' "message: $message"
    $record = Assert-InstalledBuild -Installed (Join-Path $work 'installed-0.exe') -Built $buildFile
    Report ($record -like "installed sha256 * = build sha256 $fileHash with the NSIS mark at offset $($head.Length)") 'Assert-InstalledBuild returns a record on a match' "record: $record"
} finally {
    # The only removal in this test: the temporary folder that it made above.
    Remove-Item -LiteralPath $work -Recurse -Force
}

# The install test uses the comparison, and the workflow gives it the hashed build.
$installTool = Join-Path $PSScriptRoot 'test_install.ps1'
$tokens = $null; $errors = $null
$ast = [Management.Automation.Language.Parser]::ParseFile($installTool, [ref]$tokens, [ref]$errors)
$parameter = @($ast.ParamBlock.Parameters | Where-Object { $_.Name.VariablePath.UserPath -eq 'DesktopExe' })
$mandatory = $parameter.Count -eq 1 -and @($parameter[0].Attributes | Where-Object {
    $_.TypeName.Name -eq 'Parameter' -and @($_.NamedArguments | Where-Object { $_.ArgumentName -eq 'Mandatory' -and $_.Argument.Extent.Text -eq '$true' }).Count
}).Count -eq 1
Report ($errors.Count -eq 0 -and $mandatory) 'test_install.ps1 takes the build with a mandatory -DesktopExe' "$($errors.Count) parse errors; DesktopExe parameters: $($parameter.Count)"

$function = $ast.Find({ param($node) $node -is [Management.Automation.Language.FunctionDefinitionAst] -and $node.Name -eq 'Assert-Installed' }, $true)
$calls = if ($function) { @($function.Body.FindAll({ param($node) $node -is [Management.Automation.Language.CommandAst] -and $node.GetCommandName() -eq 'Assert-InstalledBuild' }, $true)) } else { @() }
$bound = $calls.Count -eq 1 -and $calls[0].Extent.Text -match '-Built\s+\$desktopFull\b'
Report $bound 'Assert-Installed in test_install.ps1 checks the installed program against the build' "Assert-InstalledBuild calls in Assert-Installed: $($calls.Count)"

$inputs = $ast.Find({ param($node) $node -is [Management.Automation.Language.ForEachStatementAst] -and $node.Condition.Extent.Text -match '\$Setup' }, $true)
Report ($inputs -and $inputs.Condition.Extent.Text -match '\$DesktopExe\b') 'test_install.ps1 refuses a missing build file before its first change' 'the input-file check does not list $DesktopExe'

$workflow = Get-Content -LiteralPath (Join-Path $PSScriptRoot '..\.github\workflows\native-proof.yml') -Raw
$runLine = @($workflow -split "`n" | Where-Object { $_ -match 'tools/test_install\.ps1 ' -and $_ -notmatch '^\s*#' })
$passes = $runLine.Count -eq 1 -and $runLine[0].Contains('-DesktopExe "$env:RUNNER_TEMP\build\akm-desktop.exe"')
$hashed = $workflow.Contains('Copy-Item target/release/akm-desktop.exe out/')
Report ($passes -and $hashed) 'the install job passes the desktop build that the build job hashed' "run lines: $($runLine.Count); build copied into the hashed folder: $hashed"

if ($failures) { Write-Output "$failures case(s) failed"; exit 1 }
Write-Output 'all cases passed'
