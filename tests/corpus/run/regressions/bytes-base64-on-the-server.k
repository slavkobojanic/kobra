// `Bytes.base64` calls `btoa`, which the server's V8 isolate doesn't have:
// encoding bytes as base64 on the server is a `ReferenceError` (a panic).
func main() -> String {
    return "hi?".utf8.base64
}
