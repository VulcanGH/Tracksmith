# Pinned, signed compiler in the ignored local tools directory; no system installation.
$ErrorActionPreference='Stop'
$repoRoot=Split-Path $PSScriptRoot -Parent
$tools=Join-Path $repoRoot '.tools'
$destination=Join-Path $tools 'innosetup'
if(Test-Path -LiteralPath (Join-Path $destination 'ISCC.exe')) { return }
New-Item -ItemType Directory -Force (Join-Path $tools 'downloads') | Out-Null
$download=Join-Path $tools 'downloads/innosetup-7.1.1-x64.exe'
Invoke-WebRequest -Uri 'https://github.com/jrsoftware/issrc/releases/download/is-7_1_1/innosetup-7.1.1-x64.exe' -OutFile $download
$signature=Get-AuthenticodeSignature -LiteralPath $download
if($signature.Status -ne 'Valid' -or $signature.SignerCertificate.Subject -notlike '*Pyrsys B.V.*') { throw 'Inno Setup publisher signature validation failed' }
$arguments='/VERYSILENT /SUPPRESSMSGBOXES /NORESTART /NOICONS /PORTABLE=1 /TASKS= /DIR="'+$destination+'"'
$process=Start-Process -FilePath $download -ArgumentList $arguments -WindowStyle Hidden -Wait -PassThru
if($process.ExitCode -ne 0 -or !(Test-Path -LiteralPath (Join-Path $destination 'ISCC.exe'))) { throw 'Local Inno Setup compiler setup failed' }
