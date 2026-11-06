// `throw` of the function's own error type needs nothing else; the thrown
// expression is an ordinary case of the error enum.
enum LoadError implements Error {
    case offline
    case notFound
}

func load() throws LoadError -> Int {
    return 1
}

func first(values: List<Int>) throws LoadError -> Int {
    guard let value = values.first else {
        throw LoadError.notFound
//            ^^^^^^^^^^^^^^^^^^ type: corpus.LoadError
//                      ^^^^^^^^ resolves: member corpus.LoadError.notFound
    }
    return value
}
