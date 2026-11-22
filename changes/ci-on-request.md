---
kind: internal
design: 48
---
CI runs only when started (`gh workflow run`), and the release workflow takes the version as an input, checks it with `cargo xtask ci check-version`, tags the gated commit itself and makes the release; `publish.yml` publishes it.
