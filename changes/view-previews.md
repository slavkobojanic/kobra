---
kind: preview
design: 57
book: apps/previews.md, apps/testing.md, reference/diagnostics.md, reference/design-status.md
---
`preview` declarations (preview feature `view-previews`): a view rendered in a chosen state, written in the file of the view, with typed parameters as its state and a body checked as test code (fixtures, fakes, `use` rules, `@Inject`). `k check` mounts each preview once with its defaults in the simulated tab the tests use and reports a failure as `K7520`; previews never reach a build. New diagnostics `K7504`–`K7509` check the declarations.
