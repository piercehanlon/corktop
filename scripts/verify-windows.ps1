# Run from the extracted, versioned installation.
$ErrorActionPreference = 'Stop'
$exe = Join-Path $PSScriptRoot 'corktop.exe'
$receipt = Get-Content -LiteralPath (Join-Path $PSScriptRoot 'build.json') -Raw | ConvertFrom-Json
$digest = (Get-FileHash -LiteralPath $exe -Algorithm SHA256).Hash.ToLowerInvariant()
if ($digest -ne $receipt.sha256) { throw 'Installed executable digest mismatch' }

function Invoke-Corktop {
    $text = & $exe @args | Out-String
    if ($LASTEXITCODE -ne 0) { throw 'Corktop self-check failed' }
    $text.Trim()
}

function Get-BoardState($board) {
    (Get-ChildItem -LiteralPath $board -Force | Sort-Object Name | ForEach-Object {
        $_.Name + '=' + (Get-FileHash -LiteralPath $_.FullName).Hash
    }) -join ','
}

$root = Join-Path ([IO.Path]::GetTempPath()) ([Guid]::NewGuid().ToString())
$null = [IO.Directory]::CreateDirectory($root)
$previous = @{}
try {
    foreach ($name in 'HOME','USERPROFILE','LOCALAPPDATA','APPDATA','TEMP','TMP') {
        $previous[$name] = [Environment]::GetEnvironmentVariable($name, 'Process')
        $fresh = Join-Path $root $name
        $null = [IO.Directory]::CreateDirectory($fresh)
        [Environment]::SetEnvironmentVariable($name, $fresh, 'Process')
    }
    $id = Invoke-Corktop --identity | ConvertFrom-Json
    $version = Invoke-Corktop --version
    if ($id.commit -ne $receipt.commit -or $id.version -ne $receipt.version -or
        $version -ne $id.version -or $id.target -ne 'x86_64-pc-windows-gnu') {
        throw 'Installed build identity mismatch'
    }
    if ((Invoke-Corktop --health | ConvertFrom-Json).health -ne 'pass' -or
        (Invoke-Corktop --ready | ConvertFrom-Json).readiness -ne 'pass') { throw 'Self-check failed' }
    $board = Join-Path $root 'board'
    $first = Invoke-Corktop --probe $board
    $marker = 'corktop-gate-2-fixture'
    $result = $first | ConvertFrom-Json
    $manifest = Join-Path $board '.board.toml'
    $state = Get-BoardState $board
    if ($result.marker -ne $marker -or $result.schema_version -ne 1 -or
        (Invoke-Corktop --probe $board) -ne $first -or (Get-BoardState $board) -ne $state) {
        throw 'Repeat persistence probe failed'
    }
    [IO.File]::WriteAllText($manifest, "schema_version=0`nmarker=`"$marker`"`nextra=37`n")
    if ((Invoke-Corktop --probe $board) -ne $first) { throw 'Migration failed' }
    $text = [IO.File]::ReadAllText($manifest)
    $state = Get-BoardState $board
    if ($text -notmatch '(?m)^schema_version\s*=\s*1\s*$' -or
        $text -notmatch '(?m)^extra\s*=\s*37\s*$' -or
        $text -notmatch '(?m)^marker\s*=\s*"corktop-gate-2-fixture"\s*$' -or
        (Invoke-Corktop --probe $board) -ne $first -or (Get-BoardState $board) -ne $state) {
        throw 'Migrated state mismatch'
    }
    $window = Start-Process -FilePath $exe -PassThru
    try {
        $answer = Read-Host "Inspect the borderless Corktop window and type the only text it displays"
        if ($window.HasExited -or $answer -ne $version) { throw 'Window check incomplete' }
    } finally {
        if (!$window.HasExited) { $window.Kill(); $window.WaitForExit() }
        $window.Dispose()
    }
    [ordered]@{version=$version; commit=$id.commit; target=$id.target; sha256=$digest;
        health='pass'; readiness='pass'; migration='pass'; repeat_probe='pass';
        window='version-only borderless'} | ConvertTo-Json -Compress
} finally {
    foreach ($name in $previous.Keys) {
        [Environment]::SetEnvironmentVariable($name, $previous[$name], 'Process')
    }
    Remove-Item -LiteralPath $root -Recurse -Force
}
