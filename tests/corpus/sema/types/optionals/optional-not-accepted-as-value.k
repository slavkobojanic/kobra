// `T?` doesn't convert to `T`: passing an optional where a value is expected is K2001.
func double(n: Int) -> Int {
    return n * 2
}

func run(maybe: Int?) -> Int {
    return double(n: maybe)
}
