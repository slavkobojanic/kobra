# Installs the Kobra toolchain, the `k` command, on Windows
# (docs/design/48-releases-and-versioning.md, "Artifacts";
# docs/compiler/release.md §12 step 11). The same steps as install.sh:
#
#   irm https://slavkobojanic.github.io/kobra/install.ps1 | iex
#   & ([scriptblock]::Create((irm https://slavkobojanic.github.io/kobra/install.ps1))) -Version 0.1.0
#
# It picks the archive for this machine's processor, downloads it and the
# release's SHA256SUMS from GitHub Releases, checks the digest, unpacks the
# toolchain into $K_HOME\toolchains\<version>\, makes it the default
# ($K_HOME\bin\k.exe, the version in $K_HOME\default) and prints how to put
# $K_HOME\bin on the PATH. Running it again changes nothing. While no
# stable release exists, it installs the newest release candidate (else
# the newest nightly) and says so.
#
# Variables: K_RELEASES (the release host, with latest/download/<file> and
# download/v<version>/<file>; the docs site's copy has it filled in),
# K_TOOLCHAIN_INDEX (the toolchain index, index.json, read to find the
# newest pre-release when there is no stable release; filled in likewise),
# K_VERSION (a version instead of the newest stable), K_HOME (where
# toolchains go; %USERPROFILE%\.k by default).

param([string]$Version = $env:K_VERSION)

$ErrorActionPreference = 'Stop'
$ProgressPreference = 'SilentlyContinue'

function Fail([string]$Message) {
    [Console]::Error.WriteLine("error: $Message")
    exit 1
}

$Releases = if ($env:K_RELEASES) { $env:K_RELEASES } else { '@K_RELEASES@' }
if ($Releases.StartsWith('@')) {
    Fail 'this copy of install.ps1 names no release host (none is published yet); set K_RELEASES'
}
$Releases = $Releases.TrimEnd('/')
$Index = if ($env:K_TOOLCHAIN_INDEX) { $env:K_TOOLCHAIN_INDEX } else { '@K_TOOLCHAIN_INDEX@' }
$KHome = if ($env:K_HOME) { $env:K_HOME } else { Join-Path $env:USERPROFILE '.k' }

$Arch = if ($env:K_INSTALL_ARCH) { $env:K_INSTALL_ARCH } else { $env:PROCESSOR_ARCHITECTURE }
switch ($Arch) {
    'AMD64' { $Target = 'x86_64-pc-windows-msvc' }
    'ARM64' { $Target = 'aarch64-pc-windows-msvc' }
    default { Fail "Kobra releases no toolchain for the $Arch processor; build k from source" }
}

$From = if ($Version) { "$Releases/download/v$($Version.TrimStart('v'))" } else { "$Releases/latest/download" }

# The newest release in the toolchain index that isn't yanked and has an
# archive for $Target: a stable release before a candidate before a
# nightly. Returns @(version, channel), or $null.
function Get-Newest([string]$Path) {
    $Best = $null
    foreach ($Release in (Get-Content -Raw $Path | ConvertFrom-Json).releases) {
        if ($Release.yanked) { continue }
        if (-not ($Release.archives | Where-Object { $_.target -eq $Target })) { continue }
        if ($Release.version -notmatch '^(\d+)\.(\d+)\.(\d+)(?:-(rc|nightly)\.(\d+))?$') { continue }
        $Rank = switch ($Matches[4]) { 'rc' { 2 } 'nightly' { 1 } default { 3 } }
        $Num = if ($Matches[5]) { [long]$Matches[5] } else { 0 }
        $Key = '{0} {1:D9} {2:D9} {3:D9} {4:D12}' -f $Rank, [long]$Matches[1], [long]$Matches[2], [long]$Matches[3], $Num
        if (-not $Best -or $Key -gt $Best[0]) {
            $Channel = switch ($Rank) { 3 { 'stable' } 2 { 'candidate' } default { 'nightly' } }
            $Best = @($Key, $Release.version, $Channel)
        }
    }
    if ($Best) { return @($Best[1], $Best[2]) }
    return $null
}

New-Item -ItemType Directory -Force -Path (Join-Path $KHome 'tmp') | Out-Null
$Work = Join-Path (Join-Path $KHome 'tmp') ("install-ps1." + [System.Guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Path $Work | Out-Null
try {
    $Sums = Join-Path $Work 'SHA256SUMS'
    try { Invoke-WebRequest -UseBasicParsing -Uri "$From/SHA256SUMS" -OutFile $Sums }
    catch {
        if ($Version) { Fail "cannot download $From/SHA256SUMS" }
        # GitHub's `latest` skips pre-releases: with only release candidates
        # (or nightlies) published there is none, and the index names them.
        $NoStable = "there is no stable release at $Releases yet; install a pre-release by name with -Version <version>"
        if ($Index.StartsWith('@')) { Fail $NoStable }
        $IndexFile = Join-Path $Work 'index.json'
        try { Invoke-WebRequest -UseBasicParsing -Uri $Index -OutFile $IndexFile } catch { Fail $NoStable }
        $Chosen = Get-Newest $IndexFile
        if (-not $Chosen) { Fail $NoStable }
        $Version, $Channel = $Chosen
        if ($Channel -eq 'candidate') {
            Write-Output "note: there is no stable release of Kobra yet; installing the newest release candidate, $Version (choose another with -Version <version>)"
        } elseif ($Channel -eq 'nightly') {
            Write-Output "note: there is no stable release or release candidate of Kobra yet; installing the newest nightly, $Version (choose another with -Version <version>)"
        }
        $From = "$Releases/download/v$Version"
        try { Invoke-WebRequest -UseBasicParsing -Uri "$From/SHA256SUMS" -OutFile $Sums }
        catch { Fail "cannot download $From/SHA256SUMS" }
    }
    $Suffix = "-$Target.zip"
    $Line = Get-Content $Sums | ForEach-Object {
        $Parts = $_ -split '\s+', 2
        $Name = $Parts[1].TrimStart('*')
        if ($Name.StartsWith('k-') -and $Name.EndsWith($Suffix)) { , @($Parts[0].ToLower(), $Name) }
    } | Select-Object -First 1
    if (-not $Line) { Fail "the release at $From has no archive for $Target" }
    $Digest, $Archive = $Line
    $Found = $Archive.Substring(2, $Archive.Length - 2 - $Suffix.Length)

    $Dest = Join-Path (Join-Path $KHome 'toolchains') $Found
    if (Test-Path (Join-Path $Dest 'k.exe')) {
        Write-Output "k $Found is already installed in $Dest"
    } else {
        $File = Join-Path $Work $Archive
        try { Invoke-WebRequest -UseBasicParsing -Uri "$From/$Archive" -OutFile $File }
        catch { Fail "cannot download $From/$Archive" }
        $Got = (Get-FileHash -Algorithm SHA256 $File).Hash.ToLower()
        if ($Got -ne $Digest) {
            Fail "checksum mismatch for ${Archive}: SHA256SUMS says $Digest, the download is $Got; nothing was installed"
        }
        $Unpacked = Join-Path $Work 'unpacked'
        Expand-Archive -Path $File -DestinationPath $Unpacked
        $Src = $Unpacked
        if (-not (Test-Path (Join-Path $Src 'k.exe'))) { $Src = Join-Path $Unpacked "k-$Found-$Target" }
        if (-not (Test-Path (Join-Path $Src 'k.exe'))) { Fail "$Archive holds no k.exe; nothing was installed" }
        New-Item -ItemType Directory -Force -Path (Join-Path $KHome 'toolchains') | Out-Null
        if (Test-Path $Dest) { Remove-Item -Recurse -Force $Dest }
        Move-Item $Src $Dest
        Write-Output "installed k $Found ($Target) in $Dest"
    }

    # The default: a copy in bin\. A running k.exe can't be overwritten,
    # but it can be renamed, so the old one is moved aside first.
    $Bin = Join-Path $KHome 'bin'
    New-Item -ItemType Directory -Force -Path $Bin | Out-Null
    $Exe = Join-Path $Bin 'k.exe'
    $New = Join-Path $Bin '.k.new.exe'
    Copy-Item (Join-Path $Dest 'k.exe') $New -Force
    if (Test-Path $Exe) {
        $Old = Join-Path $Bin 'k.old'
        if (Test-Path $Old) { Remove-Item -Force $Old -ErrorAction SilentlyContinue }
        Move-Item -Force $Exe $Old
    }
    Move-Item -Force $New $Exe
    Set-Content -NoNewline -Path (Join-Path $KHome 'default') -Value "$Found`n"
    Write-Output "$Exe is k $Found"

    $OnPath = ($env:PATH -split ';') -contains $Bin
    if (-not $OnPath) {
        Write-Output "add k to your PATH, for your user, with:"
        Write-Output ""
        Write-Output "    [Environment]::SetEnvironmentVariable('PATH', `"$Bin;`" + [Environment]::GetEnvironmentVariable('PATH', 'User'), 'User')"
    }
} finally {
    Remove-Item -Recurse -Force $Work -ErrorAction SilentlyContinue
}
