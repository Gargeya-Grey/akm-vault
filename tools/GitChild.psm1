<#
.SYNOPSIS
Start Git for a read of the developer's own working repository, with no inherited GIT_ variable,
no fetch, no transport and no program of the repository's own (AGENTS.md class F, findings U4-R1-01,
U4-R2-01).

.DESCRIPTION
The PowerShell twin of tools/git_child.py. Every PowerShell tool in tools/ and .agents/ that starts
Git does it through Invoke-CleanGit, and tools/test_git_child.py fails when one does not.

Invoke-CleanGit removes every GIT_ variable from this process for the length of one Git call
(GIT_DIR, GIT_WORK_TREE, GIT_INDEX_FILE, GIT_CONFIG_* and the rest), and sets the same variables and
command-line settings as git_child.py: GIT_OPTIONAL_LOCKS=0 (a read never writes the index),
GIT_NO_LAZY_FETCH=1 (a missing object is an error, never a fetch), GIT_ALLOW_PROTOCOL=/ (no
transport), GIT_NO_REPLACE_OBJECTS=1, GIT_TERMINAL_PROMPT=0, and -c core.fsmonitor=false,
-c core.hooksPath=NUL, -c protocol.allow=never. GIT_CEILING_DIRECTORIES stops Git from looking
above the named folder. Then it puts the old values back.

It is for reads of the developer's own working repository only: the verification doctor reads HEAD,
the branch and the status of its own checkout. A tool that reads a commit's objects uses
git_child.source_copy instead.

The Git arguments are one array, for example Invoke-CleanGit $repo @('status', '--porcelain'). With
loose arguments PowerShell binds a word such as -p to its own common parameter -PipelineVariable and
drops a lone --, so Git would not receive what the caller wrote.
#>

$script:GitSettings = @{
    GIT_OPTIONAL_LOCKS     = '0'
    GIT_NO_LAZY_FETCH      = '1'
    GIT_ALLOW_PROTOCOL     = '/'
    GIT_NO_REPLACE_OBJECTS = '1'
    GIT_TERMINAL_PROMPT    = '0'
}

function Invoke-CleanGit {
    param(
        [Parameter(Mandatory = $true, Position = 0)][string]$Repository,
        [Parameter(Mandatory = $true, Position = 1)][string[]]$Arguments
    )
    $folder = (Resolve-Path -LiteralPath $Repository).Path
    $saved = @{}
    foreach ($item in @(Get-ChildItem Env: | Where-Object { $_.Name -like 'GIT_*' })) {
        $saved[$item.Name] = $item.Value
        [Environment]::SetEnvironmentVariable($item.Name, $null, 'Process')
    }
    $owned = @($script:GitSettings.Keys) + 'GIT_CEILING_DIRECTORIES'
    foreach ($name in $script:GitSettings.Keys) { [Environment]::SetEnvironmentVariable($name, $script:GitSettings[$name], 'Process') }
    [Environment]::SetEnvironmentVariable('GIT_CEILING_DIRECTORIES', (Split-Path -Parent $folder), 'Process')
    try {
        & git.exe -c core.fsmonitor=false -c core.hooksPath=NUL -c protocol.allow=never -C $folder @Arguments
    } finally {
        foreach ($name in $owned) { [Environment]::SetEnvironmentVariable($name, $null, 'Process') }
        foreach ($name in $saved.Keys) { [Environment]::SetEnvironmentVariable($name, $saved[$name], 'Process') }
    }
}

Export-ModuleMember -Function Invoke-CleanGit
