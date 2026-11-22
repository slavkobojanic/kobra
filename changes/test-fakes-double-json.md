---
kind: fix
---
The test fakes now write a `Double` as a JSON number. `TestHttpClient.serveJson`, `MemoryStorage`, the in-memory collections and `RecordingJobQueue` used to encode it as an object, so decoding the value back failed with `expected a number`.
