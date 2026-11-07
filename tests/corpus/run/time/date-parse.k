// `Date.parse` reads RFC 3339 timestamps with offsets and fractions, and rejects other text.
func main() -> List<Int?> {
    let utc = Date.parse("2026-01-01T12:00:00Z")
    let offset = Date.parse("2026-01-01T13:00:00+01:00")
    let invalid = Date.parse("yesterday")
    let dateOnly = Date.parse("2026-01-01")
    return [utc?.unixSeconds, offset?.unixSeconds, invalid?.unixSeconds, dateOnly?.unixSeconds]
}
