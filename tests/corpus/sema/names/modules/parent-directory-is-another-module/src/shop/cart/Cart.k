// A parent directory's package-private declarations are out of scope: K1202, and `price.cents` reports nothing more.
func total(price: Price) -> Int {
    return price.cents
}
