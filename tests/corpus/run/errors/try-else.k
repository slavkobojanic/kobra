// `try … else` turns any error into a fallback value.
enum LookupError implements Error {
    case missing
}

func find(key: String) throws LookupError -> Int {
    if key == "a" {
        return 1
    }
    throw LookupError.missing
}

func main() -> List<Int> {
    let found = try find(key: "a") else { 0 }
    let fallback = try find(key: "b") else { -1 }
    return [found, fallback]
}
