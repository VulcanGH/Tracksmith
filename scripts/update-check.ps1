# Narrow offline updater validation. No network, UAC, installer or real application replacement.
$ErrorActionPreference='Stop'
$root=Split-Path $PSScriptRoot -Parent
$temporary=Join-Path ([IO.Path]::GetTempPath()) ('tracksmith-update-check-'+[guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Path $temporary | Out-Null
try {
    . (Join-Path $root 'packaging/update.ps1') -Repository 'VulcanGH/tracksmith' -Current '0.7.0' -Application (Join-Path $temporary 'tracksmith.exe')
    function Expect-Failure([scriptblock]$Action) { $failed=$false; try { & $Action | Out-Null } catch { $failed=$true }; if(!$failed){throw 'Expected updater validation to refuse the input'} }
    foreach($invalid in @('v1.0.0','01.0.0','1.0','1.0.0-rc.1','65536.0.0')) { Expect-Failure { Assert-Version $invalid } }
    Assert-Version '1.10.0'
    $release=[pscustomobject]@{draft=$false;prerelease=$false;tag_name='v1.0.0';body='Test';assets=@([pscustomobject]@{
        name='Tracksmith-1.0.0-portable.zip';browser_download_url='https://github.com/VulcanGH/tracksmith/releases/download/v1.0.0/Tracksmith-1.0.0-portable.zip';digest=('sha256:'+('a'*64));size=123
    })}
    $selected=Select-Release $release
    if($selected.version -ne '1.0.0' -or $selected.kind -ne 'portable'){throw 'Stable portable update was not selected'}
    $release.assets[0].browser_download_url='https://example.com/Tracksmith-1.0.0-portable.zip'
    Expect-Failure { Select-Release $release }
    $release.assets[0].browser_download_url=$selected.url
    $release.assets[0].digest='sha256:bad'; Expect-Failure { Select-Release $release }
    $release.assets[0].digest='sha256:'+('a'*64)
    $release.prerelease=$true; if(Select-Release $release){throw 'Prerelease was offered'}; $release.prerelease=$false
    $release.tag_name='v0.6.0'; if(Select-Release $release){throw 'Downgrade was offered'}; $release.tag_name='v1.0.0'
    [IO.File]::WriteAllText((Join-Path $temporary '.tracksmith-installed'),'marker')
    $release.assets[0].name='Tracksmith-1.0.0-Setup.exe'
    $release.assets[0].browser_download_url='https://github.com/VulcanGH/tracksmith/releases/download/v1.0.0/Tracksmith-1.0.0-Setup.exe'
    if((Select-Release $release).kind -ne 'installed'){throw 'Installed update package not selected'}
    Remove-Item -LiteralPath (Join-Path $temporary '.tracksmith-installed')
    $sample=Join-Path $temporary 'sample'
    [IO.File]::WriteAllText($sample,'original')
    $hash=(Get-FileHash -LiteralPath $sample -Algorithm SHA256).Hash
    Assert-Hash $sample $hash (Get-Item $sample).Length
    [IO.File]::WriteAllText($sample,'tampered'); Expect-Failure { Assert-Hash $sample $hash 8 }
    Add-Type -AssemblyName System.IO.Compression
    Add-Type -AssemblyName System.IO.Compression.FileSystem
    $archive=Join-Path $temporary 'test.zip'
    $zip=[IO.Compression.ZipFile]::Open($archive,[IO.Compression.ZipArchiveMode]::Create)
    try {
        foreach($name in @('tracksmith.exe','../escape.exe')) { $entry=$zip.CreateEntry($name);$stream=$entry.Open();try{$bytes=[Text.Encoding]::ASCII.GetBytes('test payload');$stream.Write($bytes,0,$bytes.Length)}finally{$stream.Dispose()} }
    } finally { $zip.Dispose() }
    $extracted=Join-Path $temporary 'payload.exe'
    Expand-Portable $archive $extracted
    if([IO.File]::ReadAllText($extracted) -ne 'test payload'){throw 'Portable entry extraction failed'}
    if(Test-Path -LiteralPath (Join-Path (Split-Path $temporary -Parent) 'escape.exe')){throw 'Archive traversal was extracted'}
    [IO.File]::WriteAllText($Application,'original portable executable')
    $Target='1.0.0'
    Expect-Failure { Install-Portable $extracted }
    if([IO.File]::ReadAllText($Application) -ne 'original portable executable'){throw 'Portable failure did not restore the original'}
    Write-Output 'PASS: stable versions, release source/asset/hash checks, installed/portable selection, archive path isolation and portable rollback (offline).'
} finally {
    $resolved=[IO.Path]::GetFullPath($temporary)
    $expected=[IO.Path]::GetFullPath([IO.Path]::GetTempPath()).TrimEnd('\')+'\'
    if($resolved.StartsWith($expected,[StringComparison]::OrdinalIgnoreCase) -and (Split-Path $resolved -Leaf) -match '^tracksmith-update-check-[a-f0-9]{32}$') { Remove-Item -LiteralPath $resolved -Recurse -Force }
}
