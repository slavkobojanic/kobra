// `throw .case` resolves against the function's `throws` type.
enum LookupError implements Error {
    case missing
}

func find(id: Int) throws LookupError -> Int {
    throw .missing
    //     ^^^^^^^ resolves: member corpus.LookupError.missing
}
