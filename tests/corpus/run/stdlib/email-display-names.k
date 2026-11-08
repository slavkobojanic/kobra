// `EmailAddress` with a display name (31-email.md), in both runtimes:
// `parse` accepts `Name <address>`, normalizes it like a bare address
// (trimmed, the domain lower-cased, the name unquoted unless it needs
// quotes), and `address` and `displayName` are its parts. A literal is
// the same value as the parsed text.
import k.text.EmailAddress

func parts(_ address: EmailAddress?) -> String {
    guard let address else {
        return "nil"
    }
    return "{address} | {address.address} | {address.displayName ?? "nil"}"
}

func main() -> List<String> {
    let shop = EmailAddress("K Shop <orders@Shop.EXAMPLE>")
    return [
        parts(EmailAddress.parse("  Ana   Doe <ana@Example.COM> ")),
        parts(EmailAddress.parse("\"Doe, Ana\" <ana@example.com>")),
        parts(EmailAddress.parse("ana@example.com")),
        parts(EmailAddress.parse("Doe, Ana <ana@example.com>")),
        parts(shop),
        "{shop == EmailAddress.parse("K Shop <orders@shop.example>")}",
    ]
}
