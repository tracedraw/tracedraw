<#
Brings this fork's own branches up to date with upstream TraceDraw
(origin/main), then builds, so new upstream releases keep carrying our
patches (external-panel: PR #4188; native-plugins: fork-only, stacked on
top of external-panel).

What it does, per branch, in order (native-plugins depends on
external-panel, so it is rebased onto the *new* external-panel, not onto
origin/main directly):
  1. git fetch origin
  2. git rebase origin/main (external-panel), then
     git rebase external-panel (native-plugins)
  3. On a clean rebase: cargo build --release -p tracedraw --bin tracedraw
  4. If that builds: cargo test --release -p tracedraw --lib (the test
     filters this script passes)

Never pushes anywhere and never force-pushes. A conflicted rebase is
aborted (nothing is left half-done) and reported by name: that branch
needs a human to resolve it once, by hand, the normal way
(`git checkout <branch>; git rebase origin/main`, fix, continue). A
rebase rewrites our 1-2 small commits on each branch; that's local
history, recoverable from `git reflog` if anything looks wrong, and
fine to redo.

Run it whenever you want to pick up a new upstream release:
  tools\update-fork.ps1

Note: git and cargo write their normal progress to stderr; this script
never redirects it (PowerShell would otherwise turn each line into a
terminating error) and checks $LASTEXITCODE after every call instead.
#>
param([string]$Repo = (Split-Path -Parent $PSScriptRoot))

Set-Location $Repo
$branches = @(
    @{ Name = 'external-panel'; Base = 'origin/main' },
    @{ Name = 'native-plugins'; Base = 'external-panel' }
)

function Section($title) { Write-Host "`n== $title ==" -ForegroundColor Cyan }

# This Windows git has core.autocrlf=true globally, which rewrites every
# checked-out text file to CRLF; a few of this repo's own hardcoded LF
# searches (e.g. help.rs's own test) then fail on a fresh checkout. Off
# for this repo only (not --global): checkouts then give back exactly what
# is stored (LF for files stored as LF), which is what the build expects.
git config core.autocrlf false

Section 'Fetching upstream'
git fetch origin --quiet
if ($LASTEXITCODE -ne 0) { Write-Host 'git fetch failed.' -ForegroundColor Red; exit 1 }
git log -1 --format='upstream origin/main is now %h %s' origin/main

$needsAttention = @()
$rebased = @()

foreach ($b in $branches) {
    Section "Rebasing $($b.Name) onto $($b.Base)"
    git checkout --quiet $b.Name
    $before = git rev-parse HEAD
    git rebase $b.Base
    if ($LASTEXITCODE -ne 0) {
        git rebase --abort
        Write-Host "CONFLICT on $($b.Name): needs a manual rebase." -ForegroundColor Yellow
        $needsAttention += $b.Name
        continue
    }
    $after = git rev-parse HEAD
    if ($before -ne $after) {
        Write-Host "$($b.Name): rebased cleanly ($($before.Substring(0,7)) -> $($after.Substring(0,7)))."
        $rebased += $b.Name
    } else {
        Write-Host "$($b.Name): already up to date."
    }
}

if ($needsAttention.Count -eq 0) {
    Section 'Building native-plugins (the branch with everything)'
    git checkout --quiet native-plugins
    cargo build --release -p tracedraw --bin tracedraw
    if ($LASTEXITCODE -ne 0) {
        Write-Host "BUILD FAILED after the rebase - something upstream changed that our patches don't agree with anymore. Needs a look by hand." -ForegroundColor Red
        exit 1
    }
    Section 'Testing'
    cargo test --release -p tracedraw --lib -- i18n menus dialogs help plugins native_plugin external_panel settings
    if ($LASTEXITCODE -ne 0) {
        Write-Host "TESTS FAILED after the rebase. Needs a look by hand." -ForegroundColor Red
        exit 1
    }
    Write-Host "`nAll clean: target\release\tracedraw.exe is the new build on top of the latest upstream." -ForegroundColor Green
} else {
    Write-Host "`nSkipped the build: resolve the rebase on $($needsAttention -join ', ') first (checkout the branch, git rebase against its base above, fix the conflicts, git rebase --continue), then run this script again." -ForegroundColor Yellow
    exit 1
}
