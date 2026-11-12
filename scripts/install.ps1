# Installs the K toolchain on Windows (docs/design/48-releases-and-versioning.md,
# "Artifacts"; docs/compiler/release.md §12 step 11). The same steps as
# install.sh:
#
#   irm <docs site>/install.ps1 | iex
#   & ([scriptblock]::Create((irm <docs site>/install.ps1))) -Version 0.1.0
#
# It picks the archive for this machine's processor, downloads it and the
# release's SHA256SUMS, checks the digest, unpacks the toolchain into
# $K_HOME\toolchains\<version>\, makes it the default ($K_HOME\bin\k.exe,
# the version in $K_HOME\default) and prints how to put $K_HOME\bin on the
# PATH. Running it again changes nothing.
#
# Variables: K_RELEASES (the release host, with latest/download/<file> and
# download/v<version>/<file>; the docs site's copy has it filled in),
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
$KHome = if ($env:K_HOME) { $env:K_HOME } else { Join-Path $env:USERPROFILE '.k' }

$Arch = if ($env:K_INSTALL_ARCH) { $env:K_INSTALL_ARCH } else { $env:PROCESSOR_ARCHITECTURE }
switch ($Arch) {
    'AMD64' { $Target = 'x86_64-pc-windows-msvc' }
    'ARM64' { $Target = 'aarch64-pc-windows-msvc' }
    default { Fail "K releases no toolchain for the $Arch processor; build k from source" }
}

$From = if ($Version) { "$Releases/download/v$($Version.TrimStart('v'))" } else { "$Releases/latest/download" }

New-Item -ItemType Directory -Force -Path (Join-Path $KHome 'tmp') | Out-Null
$Work = Join-Path (Join-Path $KHome 'tmp') ("install-ps1." + [System.Guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Path $Work | Out-Null
try {
    $Sums = Join-Path $Work 'SHA256SUMS'
    try { Invoke-WebRequest -UseBasicParsing -Uri "$From/SHA256SUMS" -OutFile $Sums }
    catch { Fail "cannot download $From/SHA256SUMS" }
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
