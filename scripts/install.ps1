# Installs the Kobra toolchain, the `k` command, on Windows, for
# collaborators on the private repository slavkobojanic/kobra
# (docs/design/48-releases-and-versioning.md, "Private distribution";
# docs/compiler/release.md section 2). The same steps as install.sh:
#
#   gh api repos/slavkobojanic/kobra/contents/scripts/install.ps1 -H 'Accept: application/vnd.github.raw' | Out-String | iex
#   & ([scriptblock]::Create((gh api repos/slavkobojanic/kobra/contents/scripts/install.ps1 -H 'Accept: application/vnd.github.raw' | Out-String))) -Version 0.1.0
#
# The releases are private, so every download is authenticated: with the
# GitHub CLI (`gh`, signed in with `gh auth login`), else with the token in
# GH_TOKEN or GITHUB_TOKEN. It picks the archive for this machine's
# processor, downloads it and the release's SHA256SUMS, checks the digest,
# unpacks the toolchain into $K_HOME\toolchains\<version>\, makes it the
# default ($K_HOME\bin\k.exe, the version in $K_HOME\default), unpacks the
# release's book into $K_HOME\docs\<version>\ (read it with `k docs`) and
# prints how to put $K_HOME\bin on the PATH. Running it again changes
# nothing. While no stable release exists, it installs the newest release
# candidate (else the newest nightly) from the newest release's index.json
# and says so.
#
# Variables: K_VERSION (a version instead of the newest stable), K_HOME
# (where toolchains go; %USERPROFILE%\.k by default), GH_TOKEN or
# GITHUB_TOKEN (a token, used when gh isn't installed), K_INSTALL_GH (the gh
# command; `none` to use the token), K_GITHUB_API (GitHub's API, for tests).

param([string]$Version = $env:K_VERSION)

$ErrorActionPreference = 'Stop'
$ProgressPreference = 'SilentlyContinue'

function Fail([string]$Message) {
    [Console]::Error.WriteLine("error: $Message")
    exit 1
}

$Repo = 'slavkobojanic/kobra'
$Api = if ($env:K_GITHUB_API) { $env:K_GITHUB_API.TrimEnd('/') } else { 'https://api.github.com' }
$Gh = if ($null -ne $env:K_INSTALL_GH) { $env:K_INSTALL_GH } else { 'gh' }
$Token = if ($env:GH_TOKEN) { $env:GH_TOKEN } elseif ($env:GITHUB_TOKEN) { $env:GITHUB_TOKEN } else { '' }
$KHome = if ($env:K_HOME) { $env:K_HOME } else { Join-Path $env:USERPROFILE '.k' }
$SignIn = "sign in with the GitHub CLI (https://cli.github.com): ``gh auth login``, or set GH_TOKEN to a token that can read $Repo"

$UseGh = $Gh -and $Gh -ne 'none' -and (Get-Command $Gh -ErrorAction SilentlyContinue)
if (-not $UseGh -and -not $Token) {
    Fail "Kobra's repository $Repo is private, and neither gh nor a token was found: install the GitHub CLI (https://cli.github.com) and run ``gh auth login``, or set GH_TOKEN (or GITHUB_TOKEN) to a token that can read $Repo"
}

$Arch = if ($env:K_INSTALL_ARCH) { $env:K_INSTALL_ARCH } else { $env:PROCESSOR_ARCHITECTURE }
switch ($Arch) {
    'AMD64' { $Target = 'x86_64-pc-windows-msvc' }
    'ARM64' { $Target = 'aarch64-pc-windows-msvc' }
    default { Fail "Kobra releases no toolchain for the $Arch processor; build k from source" }
}

# Explains a failed API request and stops.
function Stop-Api([string]$Err) {
    if ($Err -match '404|Not Found') { Fail "GitHub answered 404: you may not have access to $Repo; $SignIn" }
    if ($Err -match '401|auth login|Bad credentials') { Fail "GitHub refused the request: you are not signed in; $SignIn" }
    Fail "cannot reach GitHub: $Err"
}

# GET an API path; $null (with -Soft) or a stop when it fails.
function Get-Api([string]$Path, [switch]$Soft) {
    if ($UseGh) {
        # Windows PowerShell turns a native command's standard error into
        # errors, which 'Stop' would make fatal.
        $ErrorActionPreference = 'Continue'
        $Out = & $Gh api -H 'Accept: application/vnd.github+json' $Path 2>&1
        $Ok = $LASTEXITCODE -eq 0
        $ErrorActionPreference = 'Stop'
        $Text = ($Out | ForEach-Object { "$_" }) -join "`n"
        if ($Ok) { return ($Text | ConvertFrom-Json) }
        if ($Soft) { return $null }
        Stop-Api $Text
    }
    try {
        return Invoke-RestMethod -UseBasicParsing -Uri "$Api/$Path" -Headers @{
            Authorization = "Bearer $Token"; Accept = 'application/vnd.github+json'; 'X-GitHub-Api-Version' = '2022-11-28'
        }
    } catch {
        if ($Soft) { return $null }
        $Code = $null
        if ($_.Exception.Response) { $Code = [int]$_.Exception.Response.StatusCode }
        Stop-Api "HTTP $Code $($_.Exception.Message)"
    }
}

$Releases = @{}
# Downloads a release's asset to $Out; $false when it isn't there.
function Get-Asset([string]$Tag, [string]$Name, [string]$Out) {
    if ($UseGh) {
        $ErrorActionPreference = 'Continue'
        & $Gh release download $Tag -R $Repo -p $Name -O $Out --clobber 2>&1 | Out-Null
        $Ok = $LASTEXITCODE -eq 0
        $ErrorActionPreference = 'Stop'
        return $Ok
    }
    if (-not $Releases.ContainsKey($Tag)) { $Releases[$Tag] = Get-Api "repos/$Repo/releases/tags/$Tag" -Soft }
    $Release = $Releases[$Tag]
    if (-not $Release) { return $false }
    $Asset = $Release.assets | Where-Object { $_.name -eq $Name } | Select-Object -First 1
    if (-not $Asset) { return $false }
    # The token goes to the API only: PowerShell drops it on the redirect
    # to the signed storage URL.
    try {
        Invoke-WebRequest -UseBasicParsing -Uri $Asset.url -OutFile $Out -Headers @{
            Authorization = "Bearer $Token"; Accept = 'application/octet-stream'
        }
        return $true
    } catch { return $false }
}

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
    # Access first, so a missing permission isn't mistaken for a missing release.
    Get-Api "repos/$Repo" | Out-Null
    if ($Version) {
        $Tag = "v$($Version.TrimStart('v'))"
    } else {
        $Latest = Get-Api "repos/$Repo/releases/latest" -Soft
        if ($Latest) {
            $Tag = $Latest.tag_name
        } else {
            # GitHub's `latest` skips pre-releases: with only release
            # candidates (or nightlies) published there is none, and the
            # newest release's index names them.
            $NoStable = 'there is no stable release of Kobra yet; install a pre-release by name with -Version <version>'
            $List = Get-Api "repos/$Repo/releases?per_page=100"
            $WithIndex = $null
            # `foreach`, not the pipeline: Windows PowerShell passes a JSON
            # array down a pipeline as one object.
            foreach ($R in $List) {
                if ($R.draft -or -not $R.published_at) { continue }
                if (-not ($R.assets | Where-Object { $_.name -eq 'index.json' })) { continue }
                if (-not $WithIndex -or [datetime]$R.published_at -gt [datetime]$WithIndex.published_at) { $WithIndex = $R }
            }
            if (-not $WithIndex) { Fail $NoStable }
            $IndexFile = Join-Path $Work 'index.json'
            if (-not (Get-Asset $WithIndex.tag_name 'index.json' $IndexFile)) { Fail $NoStable }
            $Chosen = Get-Newest $IndexFile
            if (-not $Chosen) { Fail $NoStable }
            $Version, $Channel = $Chosen
            if ($Channel -eq 'candidate') {
                Write-Output "note: there is no stable release of Kobra yet; installing the newest release candidate, $Version (choose another with -Version <version>)"
            } elseif ($Channel -eq 'nightly') {
                Write-Output "note: there is no stable release or release candidate of Kobra yet; installing the newest nightly, $Version (choose another with -Version <version>)"
            }
            $Tag = "v$Version"
        }
    }
    $Sums = Join-Path $Work 'SHA256SUMS'
    if (-not (Get-Asset $Tag 'SHA256SUMS' $Sums)) {
        Fail "cannot download SHA256SUMS of $Repo's release $Tag (is it a released version?)"
    }
    $SumLines = Get-Content $Sums | ForEach-Object {
        $Parts = $_ -split '\s+', 2
        if ($Parts.Count -eq 2) { , @($Parts[0].ToLower(), $Parts[1].TrimStart('*')) }
    }
    $Suffix = "-$Target.zip"
    $Line = $SumLines | Where-Object { $_[1].StartsWith('k-') -and $_[1].EndsWith($Suffix) } | Select-Object -First 1
    if (-not $Line) { Fail "the release $Tag has no archive for $Target" }
    $Digest, $Archive = $Line
    $Found = $Archive.Substring(2, $Archive.Length - 2 - $Suffix.Length)

    $Dest = Join-Path (Join-Path $KHome 'toolchains') $Found
    if (Test-Path (Join-Path $Dest 'k.exe')) {
        Write-Output "k $Found is already installed in $Dest"
    } else {
        $File = Join-Path $Work $Archive
        if (-not (Get-Asset $Tag $Archive $File)) { Fail "cannot download $Archive from the release $Tag" }
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

    # The book, for `k docs`: optional, so a failure is only a warning.
    $BookName = "docs-$Found.tar.gz"
    $BookLine = $SumLines | Where-Object { $_[1] -eq $BookName } | Select-Object -First 1
    $Book = Join-Path (Join-Path $KHome 'docs') $Found
    if ($BookLine -and -not (Test-Path (Join-Path $Book 'index.html'))) {
        $BookFile = Join-Path $Work $BookName
        $BookDir = Join-Path $Work 'book'
        $Done = $false
        if ((Get-Asset $Tag $BookName $BookFile) -and ((Get-FileHash -Algorithm SHA256 $BookFile).Hash.ToLower() -eq $BookLine[0])) {
            New-Item -ItemType Directory -Force -Path $BookDir | Out-Null
            $ErrorActionPreference = 'Continue'
            & tar -xzf $BookFile -C $BookDir 2>&1 | Out-Null
            $ErrorActionPreference = 'Stop'
            $From = Join-Path $BookDir "docs-$Found"
            if ($LASTEXITCODE -eq 0 -and (Test-Path (Join-Path $From 'index.html'))) {
                New-Item -ItemType Directory -Force -Path (Join-Path $KHome 'docs') | Out-Null
                if (Test-Path $Book) { Remove-Item -Recurse -Force $Book }
                Move-Item $From $Book
                $Done = $true
                Write-Output "installed the book of k $Found in $Book (open it with ``k docs``)"
            }
        }
        if (-not $Done) { Write-Output "warning: the book of k $Found wasn't installed (its download, checksum or unpacking failed)" }
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
