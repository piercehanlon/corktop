# Interactive installed-copy check. Keep the disposable board available for inspection.
$ErrorActionPreference = 'Stop'
$exe = Join-Path $PSScriptRoot 'corktop.exe'
$receipt = Get-Content -Raw -LiteralPath (Join-Path $PSScriptRoot 'build.json') | ConvertFrom-Json
$digest = (Get-FileHash -LiteralPath $exe).Hash.ToLowerInvariant()
if ($digest -ne $receipt.sha256) { throw 'Installed executable digest mismatch' }
$root = Join-Path ([IO.Path]::GetTempPath()) ('corktop-strings-' + [Guid]::NewGuid())
$board = Join-Path $root 'board'
$null = [IO.Directory]::CreateDirectory($board)
Write-Host "Disposable board for this entire run: $board"

function Open-Board {
    Start-Process -FilePath $exe -ArgumentList @('--board', ('"{0}"' -f $board)) -PassThru
}
function Close-Board($process) {
    $null = Read-Host 'Click Close in corktop. After its window disappears, press Enter here'
    if (!$process.WaitForExit(5000) -or $process.ExitCode -ne 0) { throw 'Board did not close cleanly' }
}
function Inspect-Board([bool]$linked) {
    # Inspect the known two-card fixture in the format emitted by the Pilot TOML writer.
    $raw = [IO.File]::ReadAllText((Join-Path $board '.board.toml'))
    $parts = [regex]::Split($raw, '(?m)^\[\[cards\]\]\s*$')
    if ($parts.Count -ne 3) { throw 'Expected exactly two saved cards' }
    $positions = @()
    $files = @()
    $hashes = @()
    foreach ($part in $parts[1..2]) {
        $file = [regex]::Match($part, '(?m)^file = "([a-z0-9_-]+\.md)"\s*$')
        $x = [regex]::Match($part, '(?m)^x = (\d+)\s*$')
        $y = [regex]::Match($part, '(?m)^y = (\d+)\s*$')
        if (!$file.Success -or !$x.Success -or !$y.Success) { throw 'Missing card filename or position' }
        $name = $file.Groups[1].Value
        $files += $name
        $path = Join-Path $board $name
        $hashes += (Get-FileHash -LiteralPath $path).Hash
        $positions += ('{0},{1}' -f $x.Groups[1].Value,$y.Groups[1].Value)
        Write-Host "$name at $($positions[-1]): $([IO.File]::ReadAllText($path))"
    }
    if ($hashes[0] -eq $hashes[1]) { throw 'Give the two notes distinguishable contents' }
    if ($linked) {
        $ends = [regex]::Match($parts[0], '(?m)^strings\s*=\s*\[\s*\[\s*"([^"]+)"\s*,\s*"([^"]+)"\s*,?\s*\]\s*,?\s*\]\s*$')
        if (!$ends.Success -or $ends.Groups[1].Value -eq $ends.Groups[2].Value -or
            $files -cnotcontains $ends.Groups[1].Value -or $files -cnotcontains $ends.Groups[2].Value) {
            throw 'Expected one string between these two card files'
        }
    } elseif ($parts[0] -match '(?m)^strings\s*=' -and $parts[0] -notmatch '(?m)^strings\s*=\s*\[\s*\]\s*$') {
        throw 'Expected both cards with no strings'
    }
    [pscustomobject]@{ files=($files -join '|'); hashes=($hashes -join '|'); positions=($positions -join '|'); raw=$raw }
}

$previous = @{}
$app = $null
try {
    foreach ($name in 'HOME','USERPROFILE','LOCALAPPDATA','APPDATA','TEMP','TMP') {
        $previous[$name] = [Environment]::GetEnvironmentVariable($name, 'Process')
        $fresh = Join-Path $root $name
        $null = [IO.Directory]::CreateDirectory($fresh)
        [Environment]::SetEnvironmentVariable($name, $fresh, 'Process')
    }
    $id = (& $exe --identity | Out-String) | ConvertFrom-Json
    if ($LASTEXITCODE -ne 0 -or $id.commit -ne $receipt.commit -or $id.version -ne $receipt.version -or
        $id.target -ne 'x86_64-pc-windows-gnu') { throw 'Installed identity mismatch' }
    $app = Open-Board
    $null = Read-Host 'Create two different notes far apart. Double-click to type and click blank space to save each. Press Enter here once both are visible'
    $initial = Inspect-Board $false
    $null = Read-Host 'Hold the first card tack briefly, drag onto the other card, and release. Press Enter here when you see the string'
    $connected = Inspect-Board $true
    if ($connected.positions -ne $initial.positions) { throw 'Connecting changed a card position' }
    $null = Read-Host 'Drag one card by its body. Press Enter here once you see the same string follow it'
    $moved = Inspect-Board $true
    if ($initial.positions -eq $moved.positions -or $initial.files -cne $moved.files -or
        $initial.hashes -ne $moved.hashes) { throw 'Expected moved card with original Markdown contents' }
    Close-Board $app
    $app.Dispose()
    $app = $null
    $app = Open-Board
    if ((Read-Host 'Type yes if both cards reopened with the same texts, positions, and string between their tacks') -ne 'yes') {
        throw 'Restored string not confirmed'
    }
    if ($app.HasExited -or (Inspect-Board $true).raw -cne $moved.raw) { throw 'Reopened state differs' }
    $null = Read-Host 'Right-click the string and choose Cut String. Press Enter here when the connection disappears'
    $cut = Inspect-Board $false
    if ($cut.files -cne $moved.files -or $cut.hashes -ne $moved.hashes -or $cut.positions -ne $moved.positions) {
        throw 'Cut changed a card'
    }
    Close-Board $app
    $app.Dispose()
    $app = $null
    $app = Open-Board
    if ((Read-Host 'Type yes if both cards reopened intact at the same positions with the string gone') -ne 'yes') {
        throw 'Final window not confirmed'
    }
    $final = Inspect-Board $false
    if ($app.HasExited -or $final.raw -cne $cut.raw -or $final.hashes -ne $cut.hashes) { throw 'Final state differs' }
    [ordered]@{version=$id.version;commit=$id.commit;target=$id.target;sha256=$digest;cards=$final.files;
        initial=$initial.positions;restored=$final.positions;markdown='pass';string_reopened='pass';cut_reopened='pass'} | ConvertTo-Json -Compress
} finally {
    if ($app) {
        if (!$app.HasExited) { $app.Kill(); $app.WaitForExit() }
        $app.Dispose()
    }
    foreach ($name in $previous.Keys) { [Environment]::SetEnvironmentVariable($name,$previous[$name],'Process') }
    Write-Host "Board retained at $board"
}
