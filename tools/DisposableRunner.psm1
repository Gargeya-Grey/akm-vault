<#
.SYNOPSIS
The guard that keeps the native-proof tools off any computer that is not a disposable runner.

.DESCRIPTION
tools/test_install.ps1 installs and uninstalls AKM, and tools/ci_native_preflight.ps1 can install the
WebView2 runtime. Both may run only on a GitHub-hosted runner, which GitHub discards after the job
(AGENTS.md, class F: a tool must not write or delete what it does not own). The guard needs three
facts, and refuses before the tool changes anything when one is missing:

1. GITHUB_ACTIONS is 'true'.
2. RUNNER_ENVIRONMENT is 'github-hosted'. A self-hosted runner is somebody's computer and is not
   disposable, so it is refused.
3. The caller passed its explicit switch (-DisposableRunner).

The variables can be set by hand, so the guard is not a security boundary. It stops a mistake: a
person or an agent who runs the tool on their own computer. Each tool adds its own checks for
existing AKM state after this guard.
#>

function Assert-DisposableRunner {
    param(
        [Parameter(Mandatory = $true)][string]$Tool,
        [switch]$Confirmed
    )
    $why = @()
    if ($env:GITHUB_ACTIONS -cne 'true') { $why += "GITHUB_ACTIONS is '$env:GITHUB_ACTIONS', not 'true'" }
    if ($env:RUNNER_ENVIRONMENT -cne 'github-hosted') { $why += "RUNNER_ENVIRONMENT is '$env:RUNNER_ENVIRONMENT', not 'github-hosted' (a self-hosted runner is not disposable)" }
    if (-not $Confirmed) { $why += 'the switch -DisposableRunner was not passed' }
    if ($why.Count) {
        throw "REFUSED: $Tool runs only on a disposable GitHub-hosted runner, because $($why -join '; '). Nothing was changed."
    }
}

Export-ModuleMember -Function Assert-DisposableRunner
