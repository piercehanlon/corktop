# The release ZIP embeds the two earlier checks into this single script.
param([switch]$ResumeRoundup, [switch]$ResumeAfterExplorer, [string]$ResumeBoard, [switch]$ResumeDesktop, [string]$PriorBoard)
if ($ResumeDesktop -and !$ResumeBoard) { throw "Supply the retained Case board with -ResumeBoard when using -ResumeDesktop" }
if ($ResumeBoard) { $ResumeAfterExplorer = $true }
if ($ResumeAfterExplorer) { $ResumeRoundup = $true }
$ErrorActionPreference = 'Stop'
$exe = Join-Path $PSScriptRoot 'corktop.exe'
$root = Join-Path ([IO.Path]::GetTempPath()) ('corktop-v1-' + [guid]::NewGuid())
$board = Join-Path $root 'Case board'
$files = Join-Path $root 'Drop fixtures'
foreach ($dir in $board,$files) { $null = [IO.Directory]::CreateDirectory($dir) }
$previous = @{}
$app = $null
function Confirm-Step($prompt) {
    $question = $prompt
    while ($true) {
        $reply = (Read-Host "$question [yes/no]").Trim()
        if ($reply -ieq 'yes') { break }
        if ($reply -ieq 'no') { throw "Reported failure: $question" }
        Write-Host "Observation: $reply"
        $question = 'Did this check pass, including responsiveness? Enter no to stop and retain your board.'
    }
    if ($app -and $app.HasExited) { throw 'The main board closed during the check' }
}
function Quit-Board {
    $null = Read-Host 'Choose Quit from the corktop tray menu, then press Enter'
    if (!$app.WaitForExit(8000) -or $app.ExitCode -ne 0) { throw 'The main process did not quit cleanly' }
    $app.Dispose()
}
Write-Host "This run uses disposable boards under $root. It uses your clipboard for the copy and paste checks."
try {
    foreach ($name in 'LOCALAPPDATA','APPDATA') {
        $previous[$name] = [Environment]::GetEnvironmentVariable($name, 'Process')
        $path = Join-Path $root $name
        $null = [IO.Directory]::CreateDirectory($path)
        [Environment]::SetEnvironmentVariable($name, $path, 'Process')
    }
    $probe = & {
        & (Join-Path $PSScriptRoot 'verify-windows.ps1')
    } | ConvertFrom-Json
    if ($ResumeRoundup) {
        $tour = [ordered]@{status='omitted in resumed run'; previous_build='09bd3c299cfa'}
    } else {
        $tour = & {
            & (Join-Path $PSScriptRoot 'verify-everyday.ps1')
        } | ConvertFrom-Json
        if ($probe.commit -ne $tour.commit) { throw 'Build receipts differ' }
    }
    if (!$probe.commit) { throw 'Missing build identity' }
    # Use a new profile for setup after the compatibility tests.
    $env:LOCALAPPDATA = Join-Path $root 'Setup profile'
    $null = [IO.Directory]::CreateDirectory($env:LOCALAPPDATA)
    $note = Join-Path $board 'Witness.md'
    $utf8 = New-Object Text.UTF8Encoding($false)
    $manifest = Join-Path $board '.board.toml'
    if ($ResumeBoard) {
        $source = Get-Item -LiteralPath $ResumeBoard -ErrorAction Stop
        if (!(Test-Path -LiteralPath (Join-Path $source.FullName 'Witness.md')) -or
            !(Test-Path -LiteralPath (Join-Path $source.FullName '.board.toml'))) { throw 'Select the retained Case board containing Witness.md and .board.toml' }
        Get-ChildItem -LiteralPath $source.FullName -Force | Where-Object Name -ne '.corktop.lock' | ForEach-Object {
            Copy-Item -LiteralPath $_.FullName -Destination $board -Recurse -Force
        }
        $app = Start-Process $exe -ArgumentList @('--board', ('"{0}"' -f $board)) -PassThru
        Confirm-Step 'The copied Case board should show Witness and your named zone. Are both visible?'
    } else {
        [IO.File]::WriteAllText($note,"Witness`nStatement one.",$utf8)
        $app = Start-Process $exe -PassThru
        Confirm-Step "On first launch continue past the tutorial to folder setup. Choose Browse, cancel, then browse again and select $board. Stage files in Inbox. Open the on-board Inbox and pin Witness. Is the note on the board?"
        if ([IO.File]::ReadAllText($manifest) -notmatch 'file = "Witness.md"') { throw 'Inbox pin missing' }
        if (!$ResumeAfterExplorer) {
            Set-Clipboard -Value 'first copy sentinel'
            Confirm-Step 'Single-click Witness once. As soon as the card outlines, drag it a short distance. Did it move immediately without hanging? Paste into Notepad and check for Witness and Statement one.'
            if (((Get-Clipboard -Raw) -replace "`r`n","`n") -cne "Witness`nStatement one.") { throw 'First-copy payload differs' }
        }
        if (!$ResumeRoundup) {
            [IO.File]::WriteAllText($note,"Witness`nStatement revised.",$utf8)
            Confirm-Step 'Did the note update to Statement revised? Right-click the note and choose Move to recycle bin.'
            if ((Test-Path -LiteralPath $note) -or !(Test-Path -LiteralPath (Join-Path $board '.trash/Witness.md'))) { throw 'Recycle failed' }
            Confirm-Step 'Open the Recycle bin in the banner and restore Witness. Did its text and position return?'
            if ([IO.File]::ReadAllText($note) -notmatch 'Statement revised') { throw 'Restore failed' }
            Set-Clipboard -Value 'https://example.org/case'
            Confirm-Step 'Paste on empty board space. Did a bookmark appear?'
            $bookmarks = @(Get-ChildItem -LiteralPath $board -Filter '*.url')
            if ($bookmarks.Count -ne 1 -or [IO.File]::ReadAllText($bookmarks[0].FullName) -notmatch 'URL=https://example.org/case') { throw 'Bookmark mismatch' }
        }
        if (!$ResumeAfterExplorer) {
            foreach ($name in 'Copy.txt','Link.txt','Move.txt') { [IO.File]::WriteAllText((Join-Path $files $name),$name,$utf8) }
            Confirm-Step "Open $files in Explorer. With copy selected as the default drop mode, drag Copy.txt onto corktop, Ctrl-drag Link.txt, then Shift-drag Move.txt. Did the top-center badge show + COPY, LINK, and MOVE before release?"
            if (!(Test-Path (Join-Path $files 'Copy.txt')) -or !(Test-Path (Join-Path $files 'Link.txt')) -or (Test-Path (Join-Path $files 'Move.txt'))) { throw 'Drop source files differ' }
            foreach ($suffix in 'Copy.txt','Link.txt.corklink','Move.txt') {
                if (@(Get-ChildItem -LiteralPath $board -Filter "*_$suffix").Count -ne 1) { throw "Missing drop: $suffix" }
            }
            Confirm-Step 'Copy an image from Paint or Snipping Tool and paste on blank board space. Is its thumbnail visible? Then copy Copy.txt in Explorer and paste it on blank board space.'
            if (@(Get-ChildItem -LiteralPath $board -Filter '*.png').Count -ne 1 -or @(Get-ChildItem -LiteralPath $board -Filter '*_Copy*.txt').Count -ne 2) { throw 'Image or Explorer paste missing' }
        }
        foreach ($name in 'Choice one.txt','Choice two.txt') { [IO.File]::WriteAllText((Join-Path $files $name),$name,$utf8) }
        Confirm-Step "In Settings choose Dropped files > Ask. In Explorer select Choice one.txt and Choice two.txt from $files and drag them together. Did the board highlight and the chooser list both names? Tick Remember this choice and press Esc."
        if (@(Get-ChildItem -LiteralPath $board -Filter '*_Choice*').Count) { throw 'Cancelled drop created files' }
        Confirm-Step 'Drop the same pair again. Is Remember this choice unchecked? Tick it and choose Link. Drop the pair a third time. Did LINK appear during the drag and both links arrive immediately?'
        if (@(Get-ChildItem -LiteralPath $board -Filter '*_Choice*.corklink').Count -ne 4) { throw 'Expected four links after the two confirmed drops' }
        foreach ($name in 'Choice one.txt','Choice two.txt') {
            if (![IO.File]::Exists((Join-Path $files $name))) { throw 'Linked source disappeared' }
        }
        Confirm-Step 'Change the material kit in Settings. Open Search in the top bar, search Witness and choose it. Did the board reveal the note? Right-click its body, choose a paper swatch.'
        if ([IO.File]::ReadAllText($manifest) -notmatch '(?m)^tone = [0-3]') { throw 'Card color missing' }
        Confirm-Step 'Choose Place tack for each of three corners around Witness. Drag directly between the tacks to close the triangle, then name the zone. Is its label visible?'
    }
    function Read-ZonePositions {
        $toml = [IO.File]::ReadAllText($manifest)
        $section = [regex]::Match($toml, '(?ms)^\[geometry\][^\S\r\n]*\r?\n(.*?)(?=^\[\[?\w|\z)').Groups[1].Value
        $tuple = '\[\s*(\d+)\s*,\s*(\d+)\s*,?\s*\]'
        $rows = [regex]::Match($section, "(?m)^pins\s*=\s*\[\s*((?:$tuple\s*,?\s*)+)\]").Groups[1].Value
        $points = @([regex]::Matches($rows,$tuple))
        $zones = [regex]::Matches($toml,'(?ms)^\[\[geometry\.zones\]\](.*?)(?=^\[\[?\w|\z)')
        if (!$zones.Count) { throw 'Named zone missing' }
        $indices = [regex]::Match($zones[$zones.Count-1].Groups[1].Value,'(?m)^pins\s*=\s*\[([\d,\s]+)\]').Groups[1].Value
        foreach ($index in [regex]::Matches($indices,'\d+')) {
            $id = [int]$index.Value
            if ($id -ge $points.Count) { throw "Missing saved tack $id in $manifest" }
            [pscustomobject]@{id=$id; x=[int]$points[$id].Groups[1].Value; y=[int]$points[$id].Groups[2].Value}
        }
    }
    if (!$ResumeDesktop) {
        $beforeZone = @(Read-ZonePositions)
        if ($beforeZone.Count -lt 3) { throw "Could not read the zone boundary from $manifest" }
        Confirm-Step 'Drag the named zone label toward open space, release, and check that Witness and the boundary tacks move together.'
        $afterZone = @(Read-ZonePositions)
        Write-Host ("Saved tacks before: " + ($beforeZone | ConvertTo-Json -Compress))
        Write-Host ("Saved tacks after:  " + ($afterZone | ConvertTo-Json -Compress))
        $dx = $afterZone[0].x - $beforeZone[0].x
        $dy = $afterZone[0].y - $beforeZone[0].y
        if (!$dx -and !$dy) { throw "Zone movement was not saved in $manifest" }
        if ($beforeZone.Count -ne $afterZone.Count) { throw 'Zone boundary changed during drag' }
        for ($i=0; $i -lt $beforeZone.Count; $i++) {
            if ($afterZone[$i].id -ne $beforeZone[$i].id -or $afterZone[$i].x-$beforeZone[$i].x -ne $dx -or $afterZone[$i].y-$beforeZone[$i].y -ne $dy) { throw 'Zone tacks moved by different distances' }
        }
    }
    function Read-MoveSnapshot($stage, $folder) {
        $json = (& $exe --inspect-layout $folder | Out-String).Trim()
        if ($LASTEXITCODE -ne 0 -or !$json) { throw "Could not inspect the board in $folder" }
        [IO.File]::WriteAllText((Join-Path $root "move-$stage.json"),$json,$utf8)
        $layout = $json | ConvertFrom-Json
        $g = $layout.geometry
        Write-Host ("{0}: {1} tacks, {2} tack-to-tack strands, {3} card-to-tack strands, {4} card-to-card strands" -f $stage,$g.pins.Count,$g.dividers.Count,$g.anchors.Count,$layout.strings.Count)
        Write-Host ("Saved tack positions: " + (ConvertTo-Json -InputObject $g.pins -Depth 4 -Compress))
        return $layout
    }
    if ($PriorBoard -and $ResumeBoard) {
        if (Test-Path -LiteralPath (Join-Path $PriorBoard '.board.toml')) {
            Write-Host 'Comparing the retained source and result from the previous run.'
            $prior = Read-MoveSnapshot 'retained-source' $PriorBoard
            $result = Read-MoveSnapshot 'retained-result' $ResumeBoard
            foreach ($field in 'dividers','anchors','zones') {
                $old = ConvertTo-Json -InputObject $prior.geometry.$field -Depth 8 -Compress
                $new = ConvertTo-Json -InputObject $result.geometry.$field -Depth 8 -Compress
                if ($old -cne $new) { Write-Host "Retained $field changed from $old to $new" }
            }
        } else { Write-Host "Earlier board unavailable at $PriorBoard. Continuing with the copied board." }
    }
    $before = Read-MoveSnapshot 'before' $board
    Confirm-Step 'Right-click one existing bare tack, choose Move tack, then click a different location. Move the pointer away and click blank cork. Did only the original tack move?'
    $after = Read-MoveSnapshot 'after' $board
    if ($before.geometry.pins.Count -ne $after.geometry.pins.Count) { throw "Tack count changed. Compare move-before.json and move-after.json under $root" }
    $moved = 0
    for ($i = 0; $i -lt $before.geometry.pins.Count; $i++) {
        if ($before.geometry.pins[$i][0] -ne $after.geometry.pins[$i][0] -or $before.geometry.pins[$i][1] -ne $after.geometry.pins[$i][1]) { $moved++ }
    }
    if ($moved -ne 1) { throw "Expected one saved tack to move; found $moved. Snapshots retained under $root" }
    foreach ($field in 'dividers','anchors','zones') {
        $old = ConvertTo-Json -InputObject @($before.geometry.$field | Where-Object { $null -ne $_ }) -Depth 8 -Compress
        $new = ConvertTo-Json -InputObject @($after.geometry.$field | Where-Object { $null -ne $_ }) -Depth 8 -Compress
        if ($old -cne $new) { throw "Saved $field changed from $old to $new" }
    }
    if ((ConvertTo-Json -InputObject $before.strings -Depth 8 -Compress) -cne (ConvertTo-Json -InputObject $after.strings -Depth 8 -Compress)) { throw "Card-to-card strings changed. Snapshots retained under $root" }
    Confirm-Step 'Double-click empty top bar space to maximize. Did the window maximize?'
    Confirm-Step 'Double-click empty top bar space again. Did the window return to its previous size and position?'
    Confirm-Step 'In Settings > Board, does the folder show a normal drive or network-share path?'
    Confirm-Step 'Shrink the window until an item is off-screen. Click a numbered arrow to reveal it. Click Refit, then Ctrl+Z. Did the item return to its saved board position?'
    $desktop = Read-Host 'Enable Desktop layer in Settings > Behavior. Press Win+D and try clicking a card. Enter attached if the board is interactive, fallback if corktop reports that the host is unavailable, or failed for any other result'
    if ($desktop -notin 'attached','fallback') { throw "Desktop attachment failed: $desktop" }
    Confirm-Step 'Return to a standard window using the corktop tray menu if needed. Can you interact with the board?'
    Confirm-Step 'Use the corktop pushpin tray icon to hide and show the board. Did both actions work?'
    Confirm-Step 'Choose New panel from the tray and close that panel. Is the original board still responsive?'
    $saved = [IO.File]::ReadAllText($manifest)
    Quit-Board
    $app = Start-Process $exe -PassThru
    Confirm-Step 'Did the same board, material, note color and named zone return without replaying the tutorial?'
    if ($saved -cne [IO.File]::ReadAllText($manifest)) { throw 'Reopen changed the layout' }
    Quit-Board
    $app = $null
    [ordered]@{version=$probe.version; commit=$probe.commit; target=$probe.target; sha256=$probe.sha256;
        probes=$probe; everyday=$tour; first_copy=$(if ($ResumeAfterExplorer) {'previous RC2 run'} else {'pass'}); file_drops=$(if ($ResumeAfterExplorer) {'previous RC2 run'} else {'pass'}); ask_mode=$(if ($ResumeBoard) {'previous RC5 run'} else {'pass'}); resumed=[bool]$ResumeRoundup; setup=$(if ($ResumeBoard) {'previous RC5 run'} else {'pass'}); presentation=$(if ($ResumeBoard) {'previous RC5 run'} else {'pass'});
        named_zone=$(if ($ResumeDesktop) {'previous RC6 run'} else {'pass'}); desktop=$desktop; tray='pass'; reopen='pass'} | ConvertTo-Json -Depth 4 -Compress
} finally {
    if ($app) { try { if (!$app.HasExited) { $app.Kill(); $app.WaitForExit() } } catch [System.InvalidOperationException] {} }
    foreach ($name in $previous.Keys) { [Environment]::SetEnvironmentVariable($name, $previous[$name], 'Process') }
    Write-Host "Test boards retained at $root"
}
