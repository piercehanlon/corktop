# Run from the extracted release. All boards created here are disposable.
$ErrorActionPreference = 'Stop'
$exe = Join-Path $PSScriptRoot 'corktop.exe'
$receipt = Get-Content -Raw -LiteralPath (Join-Path $PSScriptRoot 'build.json') | ConvertFrom-Json
$digest = (Get-FileHash -LiteralPath $exe).Hash.ToLowerInvariant()
$id = (& $exe --identity | Out-String) | ConvertFrom-Json
if ($digest -ne $receipt.sha256 -or $id.commit -ne $receipt.commit -or $id.version -ne $receipt.version -or $id.target -ne 'x86_64-pc-windows-gnu') { throw 'Release identity mismatch' }
$root = Join-Path ([IO.Path]::GetTempPath()) ('corktop-everyday-' + [Guid]::NewGuid())
$board = Join-Path $root 'Fresh board'
$null = [IO.Directory]::CreateDirectory($board)
$payload = "Field note`nTrace the red thread.`nThird line."
$sentinel = 'Clipboard preservation check'
$utf8 = New-Object Text.UTF8Encoding($false)
function Open-Board { Start-Process $exe -ArgumentList @('--board', ('"{0}"' -f $board)) -PassThru }
function Confirm-View($message) {
    if ((Read-Host "$message Type yes to confirm").Trim() -ine 'yes') { throw 'Visual check incomplete' }
    if ($app.HasExited) { throw 'Board exited unexpectedly' }
}
function Close-Board {
    $null = Read-Host 'Click Close in corktop, then press Enter here'
    if (!$app.WaitForExit(5000) -or $app.ExitCode -ne 0) { throw 'Window did not close cleanly' }
    $app.Dispose()
}
function Raw-Board { [IO.File]::ReadAllText((Join-Path $board '.board.toml')) }
function Card-Names { foreach ($m in [regex]::Matches((Raw-Board), '(?m)^file = "([^"]+)"\s*$')) { $m.Groups[1].Value } }
function Card-State { ((Raw-Board) -split '(?m)^\[\[cards\]\]',2)[1] }
function Get-Pairs { foreach ($m in [regex]::Matches((Raw-Board), '\[\s*"([^"]+)"\s*,\s*"([^"]+)"\s*,?\s*\]')) { ($m.Groups[1].Value,$m.Groups[2].Value | Sort-Object) -join '|' } }
function Check-Clipboard($value) { if (((Get-Clipboard -Raw) -replace "`r`n","`n") -cne $value) { throw 'Unexpected clipboard contents' } }
function Check-Notes {
    $names = @(Card-Names)
    if ($names.Count -ne 3 -or @($names | Sort-Object -Unique).Count -ne 3) { throw 'Expected three separate notes' }
    foreach ($name in $names) {
        if (([IO.File]::ReadAllText((Join-Path $board $name)) -replace "`r`n","`n") -cne $payload) { throw "Unexpected note contents in $name" }
    }
}
Write-Host "Disposable boards retained under $root"
$app = Open-Board
try {
    Set-Clipboard -Value $payload
    Confirm-View 'Click blank space at the left and paste with Ctrl+V. Click blank space at the upper right and paste again. Double-click empty space below the second card, paste into the draft, and press Ctrl+Enter. Are all three cards visible?'
    Check-Notes
    $names = @(Card-Names)
    Set-Clipboard -Value $sentinel
    Confirm-View 'Single-click the first card body. Did the copied stamp appear? Wait a moment and paste into Notepad. Did all three lines appear?'
    Check-Clipboard $payload
    Set-Clipboard -Value $sentinel
    Confirm-View "Double-click the first card body. Did your associated editor open $($names[0]) with the expected text? Leave the editor open."
    Check-Clipboard $sentinel
    $before = Card-State
    Confirm-View 'With the editor still open, switch back to corktop and drag the left card by its body a short distance. Did it move immediately? You may now close the editor.'
    if ((Card-State) -ceq $before) { throw 'Move did not persist' }
    Check-Clipboard $sentinel
    $before = Card-State
    Confirm-View 'Hold the top-center tack of the left card briefly and drag to the upper-right card. Repeat from the left tack to the lower-right card. Are both strands visible under the pinheads with a small shadow?'
    $pairs = @(Get-Pairs)
    $keep = ($names[0],$names[1] | Sort-Object) -join '|'
    $remove = ($names[0],$names[2] | Sort-Object) -join '|'
    if ($pairs.Count -ne 2 -or $pairs -notcontains $keep -or $pairs -notcontains $remove -or (Card-State) -cne $before) { throw 'Connect changed positions or unexpected endpoints' }
    Check-Clipboard $sentinel
    $before = Card-State
    Confirm-View 'Near where the strands separate, hover just beside the lower strand. Did only that strand brighten? Right-click and choose Cut String. Is the upper strand still present?'
    $pairs = @(Get-Pairs)
    if ($pairs.Count -ne 1 -or $pairs[0] -cne $keep -or (Card-State) -cne $before) { throw 'Wrong cut or changed position' }
    Check-Notes
    $before = Raw-Board
    Close-Board
    $app = Open-Board
    Confirm-View 'Did the three cards reopen at their saved positions with only the upper strand?'
    if ((Raw-Board) -cne $before) { throw 'Fresh manifest changed on reopen' }
    Check-Notes
    Close-Board
    # Recreate and copy the two-note fixture saved by the 0.3.0 Linux Pilot.
    $fixture = Join-Path $root 'Pilot fixture'
    $null = [IO.Directory]::CreateDirectory($fixture)
    $first = '1795576858_amber.md'
    $second = '1795576859_indigo.md'
    [IO.File]::WriteAllText((Join-Path $fixture $first),"Amber`nStation one.",$utf8)
    [IO.File]::WriteAllText((Join-Path $fixture $second),"Indigo`nStation two.",$utf8)
    $manifest = "schema_version = 1`nstrings = []`n`n[[cards]]`nfile = `"$first`"`nx = 82`ny = 77`n`n[[cards]]`nfile = `"$second`"`nx = 462`ny = 332`n"
    [IO.File]::WriteAllText((Join-Path $fixture '.board.toml'),$manifest,$utf8)
    $board = Join-Path $root 'Pilot copy'
    Copy-Item -Recurse -LiteralPath $fixture -Destination $board
    $app = Open-Board
    Confirm-View 'Did the Pilot copy open with Amber and Indigo?'
    if ((Raw-Board) -cne $manifest) { throw 'Opening rewrote the Pilot layout' }
    Set-Clipboard -Value $payload
    Confirm-View 'Click empty board space and paste a third card. Is it visible?'
    $names = @(Card-Names)
    if ($names.Count -ne 3 -or $names[0] -cne $first -or $names[1] -cne $second -or
        !(Raw-Board).StartsWith($manifest) -or ([IO.File]::ReadAllText((Join-Path $board $names[2])) -replace "`r`n","`n") -cne $payload) { throw 'Pilot append failed' }
    $before = Raw-Board
    Close-Board
    $app = Open-Board
    Confirm-View 'Did the Pilot copy reopen with both original notes and the pasted card?'
    if ((Raw-Board) -cne $before) { throw 'Pilot manifest changed on reopen' }
    foreach ($name in $first,$second) {
        if ((Get-FileHash (Join-Path $fixture $name)).Hash -ne (Get-FileHash (Join-Path $board $name)).Hash) { throw 'Original Pilot note changed' }
    }
    Close-Board
    $app = $null
    [ordered]@{version=$id.version;commit=$id.commit;target=$id.target;sha256=$digest;fresh='pass';pilot_copy='pass';clipboard='pass';editor='pass';targeted_cut='pass';reopen='pass'} | ConvertTo-Json -Compress
} finally {
    if ($app) { try { if (!$app.HasExited) { $app.Kill(); $app.WaitForExit() } } catch [System.InvalidOperationException] {} }
}
