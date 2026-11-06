// Once `T` is fixed by the first argument, a later argument of another type is K2001.
func firstOr<T>(items: List<T>, fallback: T) -> T {
    return items.first ?? fallback
}

let n = firstOr(items: [1, 2], fallback: "zero")
