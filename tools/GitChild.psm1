<#
.SYNOPSIS
Start Git for a read with no inherited GIT_ variable and a named repository (AGENTS.md class F, finding U4-R1-01).

.DESCRIPTION
The PowerShell twin of tools/git_child.py. Every PowerShell tool in tools/ and .agents/ that starts
Git does it through Invoke-CleanGit, and tools/test_git_child.py fails when one does not.

Invoke-CleanGit removes every GIT_ variable from this process for the length of one Git call
(GIT_DIR, GIT_WORK_TREE, GIT_INDEX_FILE, GIT_CONFIG_* and the rest), sets GIT_OPTIONAL_LOCKS=0 so a
read does not refresh the index, sets GIT_CEILING_DIRECTORIES so that Git never looks above the named
folder, and then puts the old values back. It is for reads of an existing repository only.
#>

function Invoke-CleanGit {
    param(
        [Parameter(Mandatory = $true)][string]$Repository,
        [Parameter(ValueFromRemainingArguments = $true)][string[]]$Arguments
    )
    $folder = (Resolve-Path -LiteralPath $Repository).Path
    $saved = @{}
    foreach ($item in @(Get-ChildItem Env: | Where-Object { $_.Name -like 'GIT_*' })) {
        $saved[$item.Name] = $item.Value
        [Environment]::SetEnvironmentVariable($item.Name, $null, 'Process')
    }
    [Environment]::SetEnvironmentVariable('GIT_OPTIONAL_LOCKS', '0', 'Process')
    [Environment]::SetEnvironmentVariable('GIT_CEILING_DIRECTORIES', (Split-Path -Parent $folder), 'Process')
    try {
        & git.exe -C $folder @Arguments
    } finally {
        foreach ($name in @('GIT_OPTIONAL_LOCKS', 'GIT_CEILING_DIRECTORIES')) { [Environment]::SetEnvironmentVariable($name, $null, 'Process') }
        foreach ($name in $saved.Keys) { [Environment]::SetEnvironmentVariable($name, $saved[$name], 'Process') }
    }
}

Export-ModuleMember -Function Invoke-CleanGit
