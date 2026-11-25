# corktop architecture

corktop gives Windows and Linux users a spatial board made of ordinary files. The MIT-licensed desktop app runs offline. A board works without an account, a server, or a database.

The desktop window handles input and rendering. It redraws on input or state change. The board model holds card positions and links in logical canvas units. A storage adapter reads Markdown cards and a versioned TOML manifest from one selected folder. It writes a complete sibling file before replacing a manifest. The standard window ships first. Desktop embedding belongs in a separate platform module when that work starts.

## Trust boundaries

Files and manifests belong to the user. Treat paths inside them as untrusted. Reject absolute paths and parent traversal. Skip symlinks and hidden entries during discovery. Bound input size before parsing. A malformed board must leave existing files intact. Serialize writes to each board. Check generated names for Windows restrictions and case-insensitive collisions on both platforms.

## Operations

Emit JSON diagnostics to stderr with UTC time, level, message, component, and build SHA. Keep note contents, user paths, and machine identifiers out of logs. The installed executable reports its version and checks its own readiness without network access. See the [deployment decision](adr/0003-deployment.md) and [operator runbook](reference/operator-runbook.md).
