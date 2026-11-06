// Each function sees only its own parameters, even when the names repeat.
func first(value: Int) -> Int {
    return value
    //     ^^^^^ resolves: param value 2:12
}

func second(value: String) -> String {
    return value
    //     ^^^^^ resolves: param value 7:13
}

func third() -> Int {
    return value
}
