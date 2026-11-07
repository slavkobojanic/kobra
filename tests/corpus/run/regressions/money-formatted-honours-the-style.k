// `Money.formatted(as:)` is lowered to `money.formatted(style)`, but the
// runtime's `formatted()` ignores its argument and always uses the currency
// style, so `.decimal` (digits with the locale's separators, NumberStyle.k)
// shows a currency symbol.
func main() -> List<String> {
    return [1234.50.usd.formatted(as: .decimal), 1234.50.usd.formatted(as: .currency)]
}
