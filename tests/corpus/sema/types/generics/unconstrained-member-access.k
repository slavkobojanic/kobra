// Without a constraint, `T` has no members: K1203.
func priceOf<T>(item: T) -> Int {
    return item.price
}
