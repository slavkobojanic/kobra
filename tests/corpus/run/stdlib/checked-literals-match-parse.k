// A literal checked when you compile is the value the runtime's parser gives for the same text
// (20-standard-library.md, "Literals and parsing"): the compiler and the runtime share one
// definition of each format, display names, quoting and normalization included.
import k.crypto.Uuid
import k.text.EmailAddress

func same(_ literal: EmailAddress, _ text: String) -> String {
    return "{literal == EmailAddress.parse(text)} {literal}"
}

func main() -> List<String> {
    return [
        same(EmailAddress("  Ana@Example.COM "), "  Ana@Example.COM "),
        same(EmailAddress("  K   Shop  <orders@Shop.EXAMPLE>"), "  K   Shop  <orders@Shop.EXAMPLE>"),
        same(EmailAddress("\"Ana\" <ana@example.com>"), "\"Ana\" <ana@example.com>"),
        same(EmailAddress("\"Doe, Ana\" <ana@example.com>"), "\"Doe, Ana\" <ana@example.com>"),
        same(EmailAddress("\"Ana \\\"the\\\" Doe\" <ana@example.com>"), "\"Ana \\\"the\\\" Doe\" <ana@example.com>"),
        same(EmailAddress("J. Doe <j@doe.example>"), "J. Doe <j@doe.example>"),
        "{Uuid("0B6F2C1E-4D1A-4F8E-9A3B-2C5D6E7F8A9B") == Uuid.parse("0b6f2c1e-4d1a-4f8e-9a3b-2c5d6e7f8a9b")}",
        "{Url("https://user@[::1]:8443/p?q#f") == Url.parse("https://user@[::1]:8443/p?q#f")}",
    ]
}
