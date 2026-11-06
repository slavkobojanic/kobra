// `Result.of { try … }` turns a throwing call into a value: the call needs no
// enclosing `throws`, and the result carries the error type.
enum LoadError implements Error {
    case offline
    case notFound
}

func load() throws LoadError -> Int {
    return 1
}

func attempt() -> Result<Int, LoadError> {
    let outcome = Result.of { try load() }
//      ^^^^^^^ type: Result<Int, corpus.LoadError>
    return outcome
}
