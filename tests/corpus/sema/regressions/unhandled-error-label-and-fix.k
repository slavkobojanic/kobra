// K2203 has a label at the enclosing signature and a maybe-incorrect fix adding
// `throws` (diagnostics.md, effects).
enum E implements Error {
    case one
}

func f() throws E -> Int {
    throw .one
}

func g() -> Int {
    return try f()
}
