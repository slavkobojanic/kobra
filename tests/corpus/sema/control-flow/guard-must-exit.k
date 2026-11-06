func unwrap(value: Int?) -> Int {
    guard let value else {
        let fallback = 0
    }
    return value
}

func unwrapOrZero(value: Int?) -> Int {
    guard let value else {
        return 0
    }
    return value
}
