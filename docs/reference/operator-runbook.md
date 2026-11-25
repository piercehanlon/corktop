# Pilot operator runbook

## Repository

In GitHub Settings, keep visibility private until publication is approved. Enable automatic deletion of merged head branches. Add a main ruleset requiring a pull request and zero approvals. Block force pushes and deletion. Leave required status checks empty. Local CI supplies the merge evidence.

Use a fine-grained credential limited to this repository. Grant metadata read, contents write, issues write, and pull requests write for the bootstrap workflow. Keep the credential in the local credential store. The app needs no secret, repository variable, cloud identity, environment, DNS entry, or TLS setup. Leave Actions disabled.

## Install and verify

After the bootstrap PR merges, check out its exact main SHA on each build system. Follow [the deployment decision](../adr/0003-deployment.md). Build the release executable with the pinned tools. Compare the recorded artifact digest with the installed file using sha256sum on Linux or Get-FileHash on Windows. Select a versioned directory under the user's own application directory. Run the installed version screen and read-only verification against that executable. Supply the target, embedded commit, digest, health, readiness, window result, and repeat probe result for each platform. Exclude personal paths and detailed logs. Switch the launcher back to the saved executable if verification fails.
