// A Date with a fraction of a second encodes it in RFC 3339.
func main() -> List<Date?> {
    return [Date.parse("2026-01-01T12:00:00.250Z"), Date.parse("2026-03-01T00:00:00+02:00")]
}
