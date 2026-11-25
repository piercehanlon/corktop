# Verify persistent strings on Windows

Extract the entire ZIP. In PowerShell inside the extracted directory, run `.\verify-strings.ps1` for a guided test, then return its JSON result. The helper prints the disposable board path and retains it if a check fails.

## Manual test

Use one new empty folder for the whole test. From the extracted directory:

```powershell
$exe = (Resolve-Path .\corktop.exe).Path
& $exe --identity | Out-String
(Get-FileHash $exe).Hash
$board = Join-Path $env:TEMP ('corktop-strings-' + [guid]::NewGuid())
New-Item -ItemType Directory $board | Out-Null
function Open-Board { Start-Process $exe -ArgumentList @('--board', ('"{0}"' -f $board)) }
function Show-Board {
    $manifest = Join-Path $board '.board.toml'
    Get-Content -LiteralPath $manifest
    Select-String -LiteralPath $manifest -Pattern '^file = "([a-z0-9_-]+\.md)"$' | ForEach-Object {
        $file = $_.Matches[0].Groups[1].Value
        Write-Output $file
        Get-Content -LiteralPath (Join-Path $board $file)
    }
}
Open-Board
```

1. Double-click two separate blank spots and enter different titles and bodies. Click blank space to save each. Run `Show-Board` and retain the output.
2. Hold the first card's top-center tack for about 200 ms, drag onto the other card, and release. Drag a card by its body. Confirm the string follows it. Run `Show-Board` to see the endpoint filenames and updated positions.
3. Click **Close**, wait for the window to disappear, and run `Open-Board`. Confirm the same cards, texts, locations and string appear. Run `Show-Board`.
4. Right-click the string and choose **Cut String**. Close and reopen the same folder again. The string should be gone and both cards should remain in place. Run `Show-Board`.

Return the identity, digest, before/after coordinates, and restored and final manifest and Markdown output along with your two visual observations. Keep this disposable folder if any check fails.
