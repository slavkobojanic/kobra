// Error-generic parameters (09-errors.md): a block that can't throw is accepted
// where `() throws Err -> T` is expected (function subtyping, sema.md §9.6), so it
// must not fix `Err` to the empty error before a later block that throws `E`.
// Both calls throw `E` and check cleanly, whichever block comes first.
enum E implements Error {
    case one
}

func twice<T, Err>(a: () throws Err -> T, b: () throws Err -> T) throws Err -> T {
    let x = try a()
    return try b()
}

func fe() throws E -> Int {
    throw .one
}

func throwingFirst() throws E -> Int {
    return try twice(a: { try fe() }, b: { 1 })
}

func throwingSecond() throws E -> Int {
    return try twice(a: { 1 }, b: { try fe() })
}
