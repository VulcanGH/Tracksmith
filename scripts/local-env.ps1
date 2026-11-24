# Optional project-local tooling. Existing system tools remain the default otherwise.
$repoRoot = Split-Path $PSScriptRoot -Parent
if (Test-Path "$repoRoot/.tools/cargo/bin/cargo.exe") {
    $env:CARGO_HOME = "$repoRoot/.tools/cargo"
    $env:RUSTUP_HOME = "$repoRoot/.tools/rustup"
    $env:PATH = "$repoRoot/.tools/cargo/bin;$env:PATH"
    $env:RUSTFLAGS = '-C link-self-contained=yes'
}
$nodeDirectory = Get-ChildItem "$repoRoot/.tools/node-*-win-x64" -Directory -ErrorAction SilentlyContinue | Select-Object -First 1
if ($nodeDirectory) { $env:PATH = "$($nodeDirectory.FullName);$env:PATH" }
if (Test-Path "$repoRoot/.tools/w64devkit/bin") { $env:PATH = "$repoRoot/.tools/w64devkit/bin;$env:PATH" }
