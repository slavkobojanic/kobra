// A layer catches the inner error and throws its own enum wrapping it; patterns match through the wrapper.
enum DiskError implements Error {
    case full
    case missing(path: String)
}

enum SaveError implements Error {
    case disk(DiskError)
    case invalid
}

func write(path: String) throws DiskError {
    if path.isEmpty {
        throw DiskError.missing(path: "(empty)")
    }
    throw DiskError.full
}

func save(path: String) throws SaveError {
    if path == "bad" {
        throw SaveError.invalid
    }
    do {
        try write(path: path)
    } catch let error {
        throw SaveError.disk(error)
    }
}

func describe(path: String) -> String {
    do {
        try save(path: path)
        return "saved"
    } catch .disk(.full) {
        return "disk full"
    } catch .disk(.missing(let p)) {
        return "missing {p}"
    } catch .invalid {
        return "invalid"
    }
}

func main() -> List<String> {
    return [describe(path: "a.txt"), describe(path: ""), describe(path: "bad")]
}
