// Dates keep milliseconds: the difference of two parsed instants is exact.
func main() -> List<Int> {
    let a = Date.parse("2026-01-01T12:00:00.250Z")
    let b = Date.parse("2026-01-01T12:00:01.750Z")
    guard let a, let b else {
        return []
    }
    return [(b - a).inMilliseconds, a.unixSeconds]
}
