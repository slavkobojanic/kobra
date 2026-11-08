// `Url("…")` is checked when you compile, as `Url.parse` checks text (32-http-client.md): an
// absolute `http`, `https`, `ws` or `wss` URL with a host. Anything else is K2404, with the
// reason; a script scheme never becomes a `Url`.
let api = Url("https://api.stripe.com")
let internal = Url("http://shop.internal:8080")
let live = Url("wss://k.dev/live?room=1#top")
let local = Url("https://user@[::1]:8443/p")

let relative = Url("/icon.svg")
let script = Url("javascript://alert(1)")
let files = Url("ftp://files.example")
let spaced = Url("https://exa mple.com")
let hostless = Url("https://")
let port = Url("https://k.dev:123456")
