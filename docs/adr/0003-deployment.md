# 0003 · Verify native installs on both platforms

**Status** Proposed for Pilot.

**Context** The product is a desktop executable. Both supported systems need proof that the installed build starts.

**Options** Installing one platform leaves the other unproved. A public release is unnecessary for a private Pilot.

**Decision** Build each native artifact from the same merged main commit with the locked toolchain. Record its SHA-256 digest, version, embedded commit, and target. Install under a versioned user directory. Keep the previous executable. The operator controls both installs. Run the installed executable by its explicit path with a fresh profile and a disposable synthetic board. Check its identity, health, readiness, and version-only window. Run the persistence probe twice. It must return the same result and leave user boards untouched. The verifier may inspect the installed artifact and exercise scratch data only.

**Consequences** Both installed reports must match main before Gate 2 closes. A missing Windows or Linux report keeps the gate open. Roll back by selecting the prior version in the launcher. Keep board data in place.
