// `guard let` binds for the rest of the function and exits on the first nil.
func total(price: Int?, quantity: Int?) -> Int {
    guard let price, let quantity else {
        return -1
    }
    return price * quantity
}

func main() -> List<Int> {
    return [total(price: 2, quantity: 3), total(price: nil, quantity: 3), total(price: 2, quantity: nil)]
}
