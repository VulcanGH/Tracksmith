param([switch]$Release)
$ErrorActionPreference = 'Stop'
$repoRoot = Split-Path $PSScriptRoot -Parent
. "$PSScriptRoot/local-env.ps1"
function Invoke-Checked([scriptblock]$Command) {
    & $Command
    if ($LASTEXITCODE -ne 0) { throw "Build step failed with exit code $LASTEXITCODE" }
}
Push-Location $repoRoot
try {
    Invoke-Checked { cargo run --locked --bin export-schema }
    Push-Location frontend
    try {
        Invoke-Checked { npm.cmd ci }
        Invoke-Checked { npm.cmd run generate:api }
        Invoke-Checked { npm.cmd run build }
    } finally { Pop-Location }
    Invoke-Checked { python scripts/generate-notices.py }
    if ($Release) { Invoke-Checked { cargo build --locked --release --bin tracksmith } }
    else { Invoke-Checked { cargo build --locked --bin tracksmith } }
} finally { Pop-Location }
