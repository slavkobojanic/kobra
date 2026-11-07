// `Url.parse` and `Url.host` are built on the WHATWG `URL` class, which the
// server's V8 isolate doesn't have: the `ReferenceError` is caught, so on
// the server every `Url.parse` is `nil` and every `host` is `""`.
func main() -> List<String> {
    guard let url = Url.parse("https://example.com/a?b=1") else {
        return ["nil"]
    }

    return [url.host, url.path]
}
