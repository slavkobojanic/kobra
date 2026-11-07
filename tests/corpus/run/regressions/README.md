# Regressions: execution

Behaviour findings pinned as `run` cases ([testing.md §7](../../../../docs/compiler/testing.md#7-regression-tests)).
A case with a `.pending` file is an open finding: it fails once the bug is fixed, and then the
`.pending` file is deleted. Each case's leading comment names the bug.

| Case | Found by | Symptom |
|---|---|---|
| `url-parse-on-the-server.k` | adversarial testing (stage 3) | `Url.parse` is always `nil` and `host` always `""` on the server: the isolate has no `URL` class |
| `bytes-base64-on-the-server.k` | adversarial testing (stage 3) | `Bytes.base64` panics on the server: the isolate has no `btoa` |
| `money-formatted-is-exact.k` | adversarial testing (stage 3) | `Money.formatted` rounds amounts past 2^53 minor units through a double |
| `money-formatted-honours-the-style.k` | adversarial testing (stage 3) | `Money.formatted(as: .decimal)` formats as currency |
