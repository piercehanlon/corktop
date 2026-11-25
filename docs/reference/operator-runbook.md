# Pilot operator runbook

## Repository

In GitHub Settings, keep visibility private until publication is approved. Enable automatic deletion of merged head branches. Add a main ruleset requiring a pull request and zero approvals. Block force pushes and deletion. Leave required status checks empty. Local CI supplies the merge evidence.

Use a fine-grained credential limited to this repository. Grant metadata read, contents write, issues write, and pull requests write for the bootstrap workflow. Keep the credential in the local credential store. The app needs no secret, repository variable, cloud identity, environment, DNS entry, or TLS setup. Leave Actions disabled.

## Install and verify

After the bootstrap PR merges, check out its exact main SHA on the Linux build system. Follow [the deployment decision](../adr/0003-deployment.md). Build the release executable with the pinned tools. Compare the recorded artifact digest with the installed file using sha256sum on Linux or Get-FileHash on Windows. Select a versioned directory under the user's own application directory. Run the installed version screen and read-only verification against that executable. Supply the target, embedded commit, digest, health, readiness, window result, and repeat probe result for each platform. Exclude personal paths and detailed logs. Switch the launcher back to the saved executable if verification fails.

### Gate 2 commands

Use Python 3.11 or later, Git and rustup on the Linux build host. On Linux install a C linker and native X11, Wayland and OpenGL runtime libraries. For Linux CI, put the exact programs from tools.lock.toml on PATH, then run `make setup`, `make full` and `make ci`. Run both CI passes on the same SHA to check package reuse. Detailed logs stay under target/local-ci.

On the Linux build host fetch main and check out the agreed merge SHA. Run the following command from the checkout. It builds the release, installs under a version-and-commit directory, compares digests, and runs the installed checks. Inspect the window before typing its displayed version. Record the final JSON line.

Linux:

```sh
python3 scripts/release.py --install "$HOME/.local/share/corktop/versions"
```

To produce the Windows ZIP on Linux, install the Ubuntu GNU MinGW-w64 x86-64 POSIX compiler, headers and binutils matching tools.lock.toml. Run:

```sh
rustup target add x86_64-pc-windows-gnu
python3 scripts/release.py --windows-zip target/packages
```

On Windows, extract the entire ZIP under a user-owned directory, keeping its versioned folder. Double-click corktop.exe to open a board folder. The verifier launches the version screen with `--version-window`. To collect Gate 2 evidence, open PowerShell in the extracted folder and run:

```powershell
.\verify-windows.ps1
```

Inspect the window at the prompt and return the final JSON line. The verifier compares the installed executable with the build receipt, uses a fresh profile and disposable board, and checks migration and repeat persistence. The Windows installation needs neither Rust nor a repository checkout.

For issue #2, run `.\verify-text-cards.ps1` from PowerShell inside the extracted Windows ZIP. Follow its prompts to create and drag the test note, fully close the app, and inspect the reopened folder. Return the final JSON line. The verifier reads the Markdown file independently and checks the manifest coordinates before and after restart.

For the Pilot string round trip, run `.\verify-strings.ps1` in the extracted Windows ZIP or follow the included [manual test](../../scripts/test-strings-windows.md). Both routes retain the disposable board for inspection.

For Everyday text cards, extract the ZIP into a fresh directory and run `.\verify-everyday.ps1` there. It checks the release identity, guides the clipboard, editor, move, connect and cut exercises, and checks files and manifests across fresh and Pilot-board restarts. Return the final JSON line.
