// `EmailAddress("…")` is checked when you compile, by the parser `EmailAddress.parse` runs
// (31-email.md): a bare address, or a display name with the address in angle brackets. A literal
// it rejects is K2404 at the literal, with the reason.
import k.text.EmailAddress

let plain = EmailAddress("orders@shop.example")
let spaced = EmailAddress("  Ana@Example.COM ")
let named = EmailAddress("K Shop <orders@Shop.EXAMPLE>")
let quoted = EmailAddress("\"Doe, Ana\" <ana@example.com>")
let initials = EmailAddress("J. Doe <j@doe.example>")

let noAt = EmailAddress("nobody")
let twoAts = EmailAddress("a@@b.example")
let noDot = EmailAddress("ana@localhost")
let space = EmailAddress("ana doe@example.com")
let header = EmailAddress("ana@example.com\nBcc: x@y.example")
let list = EmailAddress("ana@example.com, bo@example.com")
let unquoted = EmailAddress("Doe, Ana <ana@example.com>")
let nameless = EmailAddress("<ana@example.com>")
let unclosed = EmailAddress("Ana <ana@example.com")
let empty = EmailAddress("")
