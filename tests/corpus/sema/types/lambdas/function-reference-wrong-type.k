// A function reference of the wrong type is K2001 at the name.
func shout(text: String) -> String {
    return text.uppercased()
}

func labels(prices: List<Int>) -> List<String> {
    return prices.map(shout)
}
