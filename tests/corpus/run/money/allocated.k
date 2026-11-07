// `allocated(into:)` splits without losing a cent; the first parts take the remainder.
func main() -> List<List<Money>> {
    return [10.00.usd.allocated(into: 3), 100.jpy.allocated(into: 3), 0.01.usd.allocated(into: 2), 6.usd.allocated(into: 1)]
}
