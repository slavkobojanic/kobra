---
kind: runtime
design: 38
book: apps/deployment.md, getting-started/debugging.md
---
`k build` makes one self-contained server executable, `dist/app`, with a minified, tree-shaken client and source maps that map back to your `.k` lines; it renders pages on the server and hydrates them, and runs under Docker, Compose and Kubernetes without other files.
