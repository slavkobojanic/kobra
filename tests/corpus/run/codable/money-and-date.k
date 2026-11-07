// Money encodes as an amount string plus currency code, and a Date as RFC 3339 in UTC.
struct Invoice implements Codable {
    let total: Money
    let issued: Date
}

func main() -> Invoice {
    return Invoice(total: 48.00.usd, issued: Date(unixSeconds: 1767268800))
}
