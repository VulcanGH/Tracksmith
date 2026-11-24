# Tracksmith update worker. Original implementation following XFold's separate-worker
# approach (MIT notice in notices/XFold-LICENSE.txt); no UFCM source is incorporated.
param(
    [ValidateSet('Check','Prepare','Apply')][string]$Mode = 'Check',
    [string]$Repository, [string]$Current, [string]$Application, [string]$DataDirectory,
    [string]$Target, [int]$WaitFor, [int]$Port = 7476, [string]$Certificate = ''
)
$ErrorActionPreference = 'Stop'
[Console]::OutputEncoding = New-Object Text.UTF8Encoding($false)
[Net.ServicePointManager]::SecurityProtocol = [Net.SecurityProtocolType]::Tls12

function Assert-Version([string]$Value) {
    if ($Value -cnotmatch '^(0|[1-9]\d{0,4})\.(0|[1-9]\d{0,4})\.(0|[1-9]\d{0,4})$' -or
        @($Value.Split('.') | Where-Object { [int]$_ -gt 65535 }).Count) { throw 'Invalid release version.' }
}
function Get-Kind {
    if (Test-Path -LiteralPath (Join-Path (Split-Path $Application -Parent) '.tracksmith-installed') -PathType Leaf) { return 'installed' }
    return 'portable'
}
function Assert-NoOtherInstances {
    $expected=[IO.Path]::GetFullPath($Application)
    foreach($candidate in [Diagnostics.Process]::GetProcessesByName([IO.Path]::GetFileNameWithoutExtension($Application))) {
        try {
            if($candidate.Id -eq $WaitFor) { continue }
            try { $path=$candidate.MainModule.FileName } catch { throw 'Close other Tracksmith copies before updating; one cannot be inspected.' }
            if([IO.Path]::GetFullPath($path) -ieq $expected) { throw 'Close other Tracksmith instances using this executable before updating.' }
        } finally { $candidate.Dispose() }
    }
}
function Read-Remote([string]$Url, [long]$Limit, [string]$Destination = '') {
    $request = [Net.HttpWebRequest]::Create($Url)
    $request.UserAgent = "Tracksmith/$Current"
    $request.Accept = 'application/vnd.github+json'
    $request.Timeout = 15000; $request.ReadWriteTimeout = 15000
    $request.MaximumAutomaticRedirections = 5
    $response = $null; $source = $null; $sink = $null
    try {
        $response = $request.GetResponse()
        if ($response.ResponseUri.Scheme -ne 'https' -or $response.ResponseUri.Host -notin @('api.github.com','github.com','release-assets.githubusercontent.com','objects.githubusercontent.com')) { throw 'Unexpected update download destination.' }
        if ($response.ContentLength -gt $Limit) { throw 'Update response exceeds its size limit.' }
        $source = $response.GetResponseStream()
        if ($Destination) { $sink = [IO.File]::Open($Destination,[IO.FileMode]::CreateNew,[IO.FileAccess]::Write,[IO.FileShare]::None) }
        else { $sink = New-Object IO.MemoryStream }
        $buffer = New-Object byte[] 65536; $total = 0L
        $elapsed = [Diagnostics.Stopwatch]::StartNew()
        while (($read = $source.Read($buffer,0,$buffer.Length)) -gt 0) {
            $total += $read
            if ($total -gt $Limit -or $elapsed.Elapsed.TotalSeconds -gt 180) { throw 'Update download exceeded its size or time limit.' }
            $sink.Write($buffer,0,$read)
        }
        if (!$Destination) { return [Text.Encoding]::UTF8.GetString($sink.ToArray()) }
    } finally {
        if ($sink) { $sink.Dispose() }; if ($source) { $source.Dispose() }; if ($response) { $response.Dispose() }
    }
}
function Select-Release($Release) {
    if ($Release.draft -or $Release.prerelease) { return }
    if ($Release.tag_name -cnotmatch '^v(.+)$') { throw 'The release has no stable version tag.' }
    $version = $Matches[1]; Assert-Version $version
    if ([version]$version -le [version]$Current) { return }
    $kind = Get-Kind
    $name = if ($kind -eq 'installed') { "Tracksmith-$version-Setup.exe" } else { "Tracksmith-$version-portable.zip" }
    $url = "https://github.com/$Repository/releases/download/v$version/$name"
    $assets = @($Release.assets | Where-Object { $_.name -ceq $name })
    if ($assets.Count -ne 1 -or $assets[0].browser_download_url -ine $url -or
        $assets[0].digest -cnotmatch '^sha256:[0-9a-f]{64}$' -or
        $assets[0].size -le 0 -or $assets[0].size -gt 268435456) { throw 'The release has no matching SHA-256 verified update package.' }
    $notes = [string]$Release.body
    if ($notes.Length -gt 4096) { $notes = $notes.Substring(0,4096) + "`n[Release notes shortened]" }
    [pscustomobject]@{ version=$version; kind=$kind; url=$url; size=[long]$assets[0].size; hash=$assets[0].digest.Substring(7); notes=$notes; release_url="https://github.com/$Repository/releases/tag/v$version" }
}
function Get-Release {
    try { $json = Read-Remote "https://api.github.com/repos/$Repository/releases/latest" 2097152 }
    catch [Net.WebException] {
        if ($_.Exception.Response -and [int]$_.Exception.Response.StatusCode -eq 404) { return [pscustomobject]@{ unpublished=$true } }
        throw 'GitHub could not be reached or its API limit was reached. Try again later.'
    }
    $selected = Select-Release ($json | ConvertFrom-Json)
    if ($selected) { return $selected }
    return [pscustomobject]@{ current=$true }
}
function Assert-Hash([string]$Path, [string]$Hash, [long]$Size) {
    if ((Get-Item -LiteralPath $Path).Length -ne $Size -or (Get-FileHash -LiteralPath $Path -Algorithm SHA256).Hash -ine $Hash) { throw 'The update package does not match the GitHub release SHA-256 and size.' }
}
function Assert-Publisher([string]$Path) {
    if (!$Certificate) { return }
    if ($Certificate -notmatch '^[0-9a-fA-F]{64}$') { throw 'Invalid update certificate pin.' }
    $signature = Get-AuthenticodeSignature -LiteralPath $Path
    if ($signature.Status -ne 'Valid' -or !$signature.SignerCertificate) { throw 'The update has no valid publisher signature.' }
    $sha = [Security.Cryptography.SHA256]::Create()
    try { $pin = [BitConverter]::ToString($sha.ComputeHash($signature.SignerCertificate.RawData)).Replace('-','') }
    finally { $sha.Dispose() }
    if ($pin -ine $Certificate) { throw 'The update was signed by an unexpected publisher.' }
}
function Expand-Portable([string]$Archive, [string]$Destination) {
    Add-Type -AssemblyName System.IO.Compression
    Add-Type -AssemblyName System.IO.Compression.FileSystem
    $zip = [IO.Compression.ZipFile]::OpenRead($Archive)
    try {
        $entries = @($zip.Entries | Where-Object { $_.FullName -ceq 'tracksmith.exe' })
        if ($entries.Count -ne 1 -or $entries[0].Length -le 0 -or $entries[0].Length -gt 134217728) { throw 'The portable package has no valid Tracksmith executable.' }
        $inputStream = $entries[0].Open()
        $outputStream = [IO.File]::Open($Destination,[IO.FileMode]::CreateNew,[IO.FileAccess]::Write,[IO.FileShare]::None)
        try {
            $buffer = New-Object byte[] 65536; $total = 0L
            while (($read = $inputStream.Read($buffer,0,$buffer.Length)) -gt 0) {
                $total += $read; if ($total -gt 134217728) { throw 'Portable executable exceeds its size limit.' }
                $outputStream.Write($buffer,0,$read)
            }
            if ($total -ne $entries[0].Length) { throw 'Portable executable length mismatch.' }
        } finally { $inputStream.Dispose(); $outputStream.Dispose() }
    } finally { $zip.Dispose() }
    Assert-Publisher $Destination
}
function Get-ApplicationVersion([string]$Path) {
    $start = New-Object Diagnostics.ProcessStartInfo
    $start.FileName=$Path; $start.Arguments='--version'; $start.UseShellExecute=$false
    $start.CreateNoWindow=$true; $start.RedirectStandardOutput=$true; $start.RedirectStandardError=$true
    $probe = [Diagnostics.Process]::Start($start)
    try {
        if (!$probe.WaitForExit(10000)) { $probe.Kill(); throw 'The update version probe timed out.' }
        $text = $probe.StandardOutput.ReadToEnd().Trim()
        if ($probe.ExitCode -ne 0 -or $text -cnotmatch '^tracksmith (.+)$') { throw 'The update executable has no valid version.' }
        $value=$Matches[1]; Assert-Version $value; return $value
    } finally { $probe.Dispose() }
}
function Install-Portable([string]$Payload) {
    $root=[IO.Path]::GetFullPath((Split-Path $Application -Parent))
    $backup=$Application+'.update-backup-'+[guid]::NewGuid().ToString('N')
    $fresh=$Application+'.update-new-'+[guid]::NewGuid().ToString('N')
    foreach($path in @($Application,$backup,$fresh)) { if ([IO.Path]::GetDirectoryName([IO.Path]::GetFullPath($path)) -ine $root) { throw 'Invalid portable update destination.' } }
    Copy-Item -LiteralPath $Payload -Destination $fresh
    $moved=$false
    try {
        Move-Item -LiteralPath $Application -Destination $backup; $moved=$true
        Move-Item -LiteralPath $fresh -Destination $Application
        if ((Get-ApplicationVersion $Application) -cne $Target) { throw 'The new executable has the wrong version.' }
    } catch {
        if ($moved) { if (Test-Path -LiteralPath $Application) { Remove-Item -LiteralPath $Application -Force }; Move-Item -LiteralPath $backup -Destination $Application }
        throw
    } finally { if(Test-Path -LiteralPath $fresh) { Remove-Item -LiteralPath $fresh -Force } }
    if(Test-Path -LiteralPath $backup) { Remove-Item -LiteralPath $backup -Force -ErrorAction SilentlyContinue }
}
function Clear-Stage {
    $root=[IO.Path]::GetFullPath((Join-Path $DataDirectory 'updates')).TrimEnd('\')+'\'
    $stage=[IO.Path]::GetFullPath($PSScriptRoot)
    if (!$stage.StartsWith($root,[StringComparison]::OrdinalIgnoreCase) -or (Split-Path $stage -Leaf) -notmatch '^[a-f0-9]{32}$') { return }
    Remove-Item -LiteralPath $stage -Recurse -Force -ErrorAction SilentlyContinue
}
# Offline validation checks dot-source the functions without starting network or installation work.
if ($MyInvocation.InvocationName -eq '.') { return }
try {
    if ($Repository -notmatch '^[A-Za-z0-9][A-Za-z0-9-]*/[A-Za-z0-9_.-]+$') { throw 'Invalid update repository.' }
    Assert-Version $Current
    if ($Mode -eq 'Check') { Get-Release | ConvertTo-Json -Compress; exit 0 }
    Assert-Version $Target
    if ([version]$Target -le [version]$Current) { throw 'Choose a newer release.' }
    $package=Join-Path $PSScriptRoot 'package.exe'
    $prepared=Join-Path $PSScriptRoot 'prepared.json'
    $portable=Join-Path $PSScriptRoot 'tracksmith.exe'
    if ($Mode -eq 'Prepare') {
        Assert-NoOtherInstances
        if ((Get-ApplicationVersion $Application) -cne $Current) { throw 'The running application changed; reopen Tracksmith.' }
        $release=Get-Release
        if ($release.version -cne $Target) { throw 'The selected update is no longer the latest release. Check again.' }
        if ($release.kind -eq 'portable') {
            $probe=Join-Path (Split-Path $Application -Parent) ('.tracksmith-write-'+[guid]::NewGuid().ToString('N'))
            try { [IO.File]::WriteAllText($probe,'probe') } catch { throw 'This folder is not writable. Use the Windows installer to update this copy.' }
            finally { if(Test-Path -LiteralPath $probe) { Remove-Item -LiteralPath $probe -Force } }
        }
        Read-Remote $release.url $release.size $package
        Assert-Hash $package $release.hash $release.size
        if ($release.kind -eq 'installed') { Assert-Publisher $package }
        else { Expand-Portable $package $portable; if ((Get-ApplicationVersion $portable) -cne $Target) { throw 'Portable update version mismatch.' } }
        $release | Add-Member -NotePropertyName application_hash -NotePropertyValue (Get-FileHash -LiteralPath $Application -Algorithm SHA256).Hash
        $release | ConvertTo-Json -Compress | Set-Content -LiteralPath $prepared -Encoding UTF8
        exit 0
    }
    if ($WaitFor -le 0 -or !$DataDirectory -or $Port -lt 1 -or $Port -gt 65535) { throw 'Update restart information is missing.' }
    $previous=Get-Process -Id $WaitFor -ErrorAction SilentlyContinue
    if ($previous -and !$previous.WaitForExit(120000)) { throw 'Tracksmith is still running; update postponed without terminating it.' }
    $result=@{success=$false;message='';version=$Target}
    try {
        Assert-NoOtherInstances
        $release=Get-Content -LiteralPath $prepared -Raw | ConvertFrom-Json
        if ($release.version -cne $Target -or (Get-FileHash -LiteralPath $Application -Algorithm SHA256).Hash -ine $release.application_hash) { throw 'The installed application changed after update preparation.' }
        Assert-Hash $package $release.hash $release.size
        if ($release.kind -eq 'installed') {
            Assert-Publisher $package
            $installer=Join-Path $PSScriptRoot 'Tracksmith-Setup.exe'
            Move-Item -LiteralPath $package -Destination $installer
            $arguments='/VERYSILENT /SUPPRESSMSGBOXES /NORESTART /NOCLOSEAPPLICATIONS /NOFORCECLOSEAPPLICATIONS /DIR="'+(Split-Path $Application -Parent)+'"'
            $setup=Start-Process -FilePath $installer -ArgumentList $arguments -Verb RunAs -WindowStyle Hidden -Wait -PassThru
            if ($setup.ExitCode -ne 0 -or (Get-ApplicationVersion $Application) -cne $Target) { throw 'Setup did not complete the requested update.' }
        } else {
            # Re-extract the verified archive so mutable staged executable bytes are not trusted.
            if(Test-Path -LiteralPath $portable) { Remove-Item -LiteralPath $portable -Force }
            Expand-Portable $package $portable
            Install-Portable $portable
        }
        $result.success=$true; $result.message="Updated to Tracksmith $Target."
    } catch { $result.message=$_.Exception.Message }
    finally {
        try { $result | ConvertTo-Json -Compress | Set-Content -LiteralPath (Join-Path $DataDirectory 'update-result.json') -Encoding UTF8 } catch { }
        if(Test-Path -LiteralPath $Application -PathType Leaf) {
            $start=New-Object Diagnostics.ProcessStartInfo
            $start.FileName=$Application; $start.Arguments="--port $Port"; $start.UseShellExecute=$false
            $start.CreateNoWindow=$true; $start.WindowStyle=[Diagnostics.ProcessWindowStyle]::Hidden
            $start.EnvironmentVariables['TRACKSMITH_CONFIG_DIR']=$DataDirectory
            [Diagnostics.Process]::Start($start) | Out-Null
        }
        Clear-Stage
    }
} catch { [Console]::Error.WriteLine($_.Exception.Message); exit 1 }
