# Run from the extracted installation. All board and profile files are disposable.
$ErrorActionPreference = 'Stop'
$exe = Join-Path $PSScriptRoot 'corktop.exe'
$receipt = Get-Content -Raw -LiteralPath (Join-Path $PSScriptRoot 'build.json') | ConvertFrom-Json
$digest = (Get-FileHash -LiteralPath $exe -Algorithm SHA256).Hash.ToLowerInvariant()
if ($digest -ne $receipt.sha256) { throw 'Installed executable digest mismatch' }

function Read-Position {
    $text = [IO.File]::ReadAllText((Join-Path $board '.board.toml'))
    $x = [regex]::Match($text, '(?m)^x\s*=\s*(\d+)\s*$')
    $y = [regex]::Match($text, '(?m)^y\s*=\s*(\d+)\s*$')
    if (!$x.Success -or !$y.Success) { throw 'Card position missing' }
    return ('{0},{1}' -f $x.Groups[1].Value, $y.Groups[1].Value)
}

function Open-TestBoard {
    Start-Process -FilePath $exe -ArgumentList @('--board', ('"{0}"' -f $board)) -PassThru
}

$root = Join-Path ([IO.Path]::GetTempPath()) ([Guid]::NewGuid().ToString())
$board = Join-Path $root 'board'
$null = [IO.Directory]::CreateDirectory($board)
$previous = @{}
$app = $null
try {
    foreach ($name in 'HOME','USERPROFILE','LOCALAPPDATA','APPDATA','TEMP','TMP') {
        $previous[$name] = [Environment]::GetEnvironmentVariable($name, 'Process')
        $fresh = Join-Path $root $name
        $null = [IO.Directory]::CreateDirectory($fresh)
        [Environment]::SetEnvironmentVariable($name, $fresh, 'Process')
    }
    $identity = (& $exe --identity | Out-String) | ConvertFrom-Json
    if ($LASTEXITCODE -ne 0 -or $identity.commit -ne $receipt.commit -or
        $identity.version -ne $receipt.version -or $identity.target -ne 'x86_64-pc-windows-gnu') {
        throw 'Installed identity mismatch'
    }
    $app = Open-TestBoard
    Write-Host 'Double-click empty space. Type these two lines, then press Ctrl+Enter.'
    Write-Host 'Field note'
    Write-Host 'Trace the red thread.'
    $null = Read-Host 'Leave corktop open and press Enter here once the card appears'
    $notes = @(Get-ChildItem -LiteralPath $board -Filter '*.md' -File)
    if ($notes.Count -ne 1) { throw 'Expected one Markdown file' }
    $expected = "Field note`nTrace the red thread."
    $text = [IO.File]::ReadAllText($notes[0].FullName).Replace("`r`n", "`n")
    if ($text -cne $expected) { throw 'Markdown text differs from the requested note' }
    Write-Host "Read from the Markdown file on disk:`n$text"
    $initial = Read-Position
    $null = Read-Host 'Drag the card to a different spot, release it, then press Enter here'
    $moved = Read-Position
    if ($initial -eq $moved) { throw 'The saved position did not change' }
    $null = Read-Host 'Click Close in corktop, then press Enter here'
    if (!$app.WaitForExit(5000) -or $app.ExitCode -ne 0) { throw 'corktop did not close cleanly' }
    $app.Dispose()
    $app = $null
    $app = Open-TestBoard
    $answer = Read-Host 'If the card is at the moved spot, type the body text you see on the reopened card'
    if ($app.HasExited -or $answer -cne 'Trace the red thread.' -or
        (Read-Position) -ne $moved -or
        [IO.File]::ReadAllText($notes[0].FullName).Replace("`r`n", "`n") -cne $expected) {
        throw 'Reopened card did not match'
    }
    [ordered]@{commit=$identity.commit; version=$identity.version; target=$identity.target;
        sha256=$digest; markdown='pass'; initial_position=$initial; restored_position=$moved;
        desktop_round_trip='pass'} | ConvertTo-Json -Compress
} finally {
    if ($app) {
        if (!$app.HasExited) { $app.Kill(); $app.WaitForExit() }
        $app.Dispose()
    }
    foreach ($name in $previous.Keys) {
        [Environment]::SetEnvironmentVariable($name, $previous[$name], 'Process')
    }
    Remove-Item -LiteralPath $root -Recurse -Force
}
