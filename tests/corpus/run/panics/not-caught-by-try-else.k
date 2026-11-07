// `try … else` falls back on typed errors only; a panic in the call still ends the program.
enum Never implements Error {
    case never
}

func divide(a: Int, b: Int) throws Never -> Int {
    if a < 0 {
        throw Never.never
    }
    return a / b
}

func main() -> Int {
    return try divide(a: 1, b: 0) else { -1 }
}
