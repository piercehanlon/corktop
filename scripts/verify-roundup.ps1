# The release ZIP embeds the two earlier checks into this single script.
param([switch]$ResumeRoundup)
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
    [IO.File]::WriteAllText($note,"Witness`nStatement one.",$utf8)
    $app = Start-Process $exe -PassThru
    Confirm-Step "On first launch continue past the tutorial to folder setup. Choose Browse, cancel, then browse again and select $board. Stage files in Inbox. Open the upper-right Inbox and pin Witness. Is the note on the board?"
    $manifest = Join-Path $board '.board.toml'
    if ([IO.File]::ReadAllText($manifest) -notmatch 'file = "Witness.md"') { throw 'Inbox pin missing' }
    Set-Clipboard -Value 'first copy sentinel'
    Confirm-Step 'Single-click Witness once. As soon as the card outlines, drag it a short distance. Did it move immediately without hanging? Paste into Notepad and check for Witness and Statement one.'
    if (((Get-Clipboard -Raw) -replace "`r`n","`n") -cne "Witness`nStatement one.") { throw 'First-copy payload differs' }
    if (!$ResumeRoundup) {
        [IO.File]::WriteAllText($note,"Witness`nStatement revised.",$utf8)
        Confirm-Step 'Did the note update to Statement revised? Hover the note and use its X to recycle it.'
        if ((Test-Path -LiteralPath $note) -or !(Test-Path -LiteralPath (Join-Path $board '.trash/Witness.md'))) { throw 'Recycle failed' }
        Confirm-Step 'Open the lower-left recycle bin and restore Witness. Did its text and position return?'
        if ([IO.File]::ReadAllText($note) -notmatch 'Statement revised') { throw 'Restore failed' }
        Set-Clipboard -Value 'https://example.org/case'
        Confirm-Step 'Paste on empty board space. Did a bookmark appear?'
        $bookmarks = @(Get-ChildItem -LiteralPath $board -Filter '*.url')
        if ($bookmarks.Count -ne 1 -or [IO.File]::ReadAllText($bookmarks[0].FullName) -notmatch 'URL=https://example.org/case') { throw 'Bookmark mismatch' }
    }
    foreach ($name in 'Copy.txt','Link.txt','Move.txt') { [IO.File]::WriteAllText((Join-Path $files $name),$name,$utf8) }
    Confirm-Step "Open $files in Explorer. With copy selected as the default drop mode, drag Copy.txt onto corktop, Ctrl-drag Link.txt, then Shift-drag Move.txt. Did the top-center badge show + COPY, LINK, and MOVE before release?"
    if (!(Test-Path (Join-Path $files 'Copy.txt')) -or !(Test-Path (Join-Path $files 'Link.txt')) -or (Test-Path (Join-Path $files 'Move.txt'))) { throw 'Drop source files differ' }
    foreach ($suffix in 'Copy.txt','Link.txt.corklink','Move.txt') {
        if (@(Get-ChildItem -LiteralPath $board -Filter "*_$suffix").Count -ne 1) { throw "Missing drop: $suffix" }
    }
    Confirm-Step 'Copy an image from Paint or Snipping Tool and paste on blank board space. Is its thumbnail visible? Then copy Copy.txt in Explorer and paste it on blank board space.'
    if (@(Get-ChildItem -LiteralPath $board -Filter '*.png').Count -ne 1 -or @(Get-ChildItem -LiteralPath $board -Filter '*_Copy*.txt').Count -ne 2) { throw 'Image or Explorer paste missing' }
    Confirm-Step 'Change the material kit in Settings. Open the top-right list, search Witness and choose it. Did the board reveal the note? Long-press its body, change its paper color, and apply.'
    if ([IO.File]::ReadAllText($manifest) -notmatch '(?m)^tone = [0-3]') { throw 'Card color missing' }
    Confirm-Step 'Place three bare tacks with the pin tool. Hold and drag between tacks to close a triangle, name the zone, then move the zone by its label. Did the labeled area move?'
    if ([IO.File]::ReadAllText($manifest) -notmatch '\[\[geometry.zones\]\]') { throw 'Named zone missing' }
    Confirm-Step 'Try Desktop mode and return to a standard window. Hide and show with the tray. Open a separate panel from the tray and close that panel. Did the original board remain responsive?'
    $desktop = Read-Host 'Which desktop mode result did you observe? Enter attached or fallback'
    if ($desktop -notin 'attached','fallback') { throw 'Desktop mode observation required' }
    Confirm-Step 'Resize the board, refit from the empty-board context menu, and check that notes and tacks stay aligned. If using a second display with different scaling, move the window across before refitting.'
    $saved = [IO.File]::ReadAllText($manifest)
    Quit-Board
    $app = Start-Process $exe -PassThru
    Confirm-Step 'Did the same board, material, note color and named zone return without replaying the tutorial?'
    if ($saved -cne [IO.File]::ReadAllText($manifest)) { throw 'Reopen changed the layout' }
    Quit-Board
    $app = $null
    [ordered]@{version=$probe.version; commit=$probe.commit; target=$probe.target; sha256=$probe.sha256;
        probes=$probe; everyday=$tour; first_copy='pass'; file_drops='pass'; resumed=[bool]$ResumeRoundup; setup='pass'; presentation='pass';
        named_zone='pass'; desktop=$desktop; tray='pass'; reopen='pass'} | ConvertTo-Json -Depth 4 -Compress
} finally {
    if ($app) { try { if (!$app.HasExited) { $app.Kill(); $app.WaitForExit() } } catch [System.InvalidOperationException] {} }
    foreach ($name in $previous.Keys) { [Environment]::SetEnvironmentVariable($name, $previous[$name], 'Process') }
    Write-Host "Test boards retained at $root"
}
