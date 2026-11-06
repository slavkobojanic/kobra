// Inside the error-generic function, calling the block is a throwing call of
// type `E`: it needs `try` (K2201), and the function must declare `throws E`.
func each<E>(items: List<Int>, _ block: (Int) throws E -> Void) throws E {
    for item in items {
        try block(item)
    }
}

func eachUnmarked<E>(items: List<Int>, _ block: (Int) throws E -> Void) throws E {
    for item in items {
        block(item)
    }
}
