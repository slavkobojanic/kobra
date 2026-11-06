// A function that calls a block of generic error type but doesn't declare
// `throws E` can't pass the error through (K2203 at the `try`).
func each<E>(items: List<Int>, _ block: (Int) throws E -> Void) {
    for item in items {
        try block(item)
    }
}
