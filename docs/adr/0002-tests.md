# 0002 · Package tests and evidence

**Status** Accepted for Pilot.

**Context** Local CI runs on a shared host. A cached success must represent the exact inputs under test.

**Options** Running all tests every time is simple. Cargo caches builds but normally runs test executables again. A small package result cache also avoids repeated tests.

**Decision** Use Rust's built-in test harness. Keep the board and persistence code in a board package, with the desktop executable in a dependent package. Use Cargo metadata to map changed files to packages and follow reverse dependencies. Changes to shared build inputs, the workspace graph, or unknown paths select all packages. Documentation takes the text-only path.

Cache only successful results after the entire package suite exits. Hash package file names and contents, transitive dependency inputs, workspace manifests, Cargo.lock, toolchain lock, target, features, and test commands. Publish cache entries by atomic rename. Report counts, duration, and reuse per package. Full runs bypass cached results. Use isolated temporary boards, fixed inputs, and a timeout for each test executable. Keep GUI display checks in installed-binary verification.

**Consequences** A cold run and a repeat on the same SHA prove reuse. CI also checks format, lint, secret exposure, and changed TODO lines under the host budget. Preserve detailed test output locally and publish only aggregate evidence.
