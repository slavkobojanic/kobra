// A function can be passed by name where a function type is expected.
func formatPrice(cents: Int) -> String {
    return "{cents / 100}"
}

func labels(prices: List<Int>) -> List<String> {
    return prices.map(formatPrice)
                    //^^^^^^^^^^^ type: (Int) -> String
                    //^^^^^^^^^^^ resolves: decl corpus.formatPrice
}
