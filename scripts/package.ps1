param([string]$Compiler, [string]$Version = '')
$ErrorActionPreference = 'Stop'
$repoRoot = Split-Path $PSScriptRoot -Parent
if (!$Version) { $Version = (Get-Content (Join-Path $repoRoot 'Cargo.toml') | Select-String '^version = "([^"]+)"').Matches[0].Groups[1].Value }
$output = Join-Path $repoRoot 'artifacts/packages'
$stage = Join-Path $output "Tracksmith-$Version-portable"
New-Item -ItemType Directory -Force $stage | Out-Null
Copy-Item -LiteralPath (Join-Path $repoRoot 'target/release/tracksmith.exe') -Destination $stage
Copy-Item -LiteralPath (Join-Path $repoRoot 'packaging/README.txt') -Destination $stage
Copy-Item -LiteralPath (Join-Path $repoRoot 'LICENSE') -Destination $stage
New-Item -ItemType Directory -Force (Join-Path $stage 'notices') | Out-Null
Copy-Item -LiteralPath (Join-Path $repoRoot 'docs/notices/XFold-LICENSE.txt') -Destination (Join-Path $stage 'notices')
Copy-Item -LiteralPath (Join-Path $repoRoot 'data/README.md') -Destination (Join-Path $stage 'notices/language-data.md')
Copy-Item -LiteralPath (Join-Path $repoRoot 'docs/notices/THIRD-PARTY-NOTICES.txt') -Destination (Join-Path $stage 'notices')
Copy-Item -LiteralPath (Join-Path $repoRoot 'docs/notices/rust-stdlib') -Destination (Join-Path $stage 'notices') -Recurse -Force
New-Item -ItemType Directory -Force (Join-Path $stage 'docs') | Out-Null
Copy-Item -LiteralPath (Join-Path $repoRoot 'docs/NAMING.md') -Destination (Join-Path $stage 'docs')
Copy-Item -LiteralPath (Join-Path $repoRoot 'docs/REAL-WORLD-TESTING.md') -Destination (Join-Path $stage 'docs')
Copy-Item -LiteralPath (Join-Path $repoRoot 'docs/UPDATES.md') -Destination (Join-Path $stage 'docs')
Compress-Archive -Path "$stage/*" -DestinationPath (Join-Path $output "Tracksmith-$Version-portable.zip") -Force
if (-not $Compiler) { $Compiler = Join-Path $repoRoot '.tools/innosetup/ISCC.exe' }
if (-not (Test-Path -LiteralPath $Compiler)) { throw 'Portable ZIP built. Supply -Compiler with an Inno Setup ISCC.exe path to build the installer.' }
& $Compiler "/DAppVersion=$Version" (Join-Path $repoRoot 'packaging/tracksmith.iss')
if ($LASTEXITCODE -ne 0) { throw 'Installer compilation failed' }
$sums = @("Tracksmith-$Version-portable.zip", "Tracksmith-$Version-Setup.exe") | ForEach-Object { $hash=Get-FileHash -LiteralPath (Join-Path $output $_) -Algorithm SHA256; "$($hash.Hash.ToLowerInvariant())  $_" }
[IO.File]::WriteAllText((Join-Path $output "Tracksmith-$Version-SHA256SUMS.txt"), ($sums -join "`n")+"`n")
Get-ChildItem -LiteralPath $output -File | ForEach-Object { $hash = Get-FileHash -LiteralPath $_.FullName -Algorithm SHA256; Write-Output "$($hash.Hash)  $($_.Name)" }
