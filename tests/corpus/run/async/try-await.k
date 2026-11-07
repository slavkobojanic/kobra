// `try await` on an async throwing function; errors are caught like synchronous ones.
enum FetchError implements Error {
    case notFound(id: Int)
}

func fetch(id: Int) async throws FetchError -> String {
    if id > 2 {
        throw FetchError.notFound(id: id)
    }
    return "item {id}"
}

func main() async -> List<String> {
    var out: List<String> = []
    for id in 1...3 {
        do {
            out.append(try await fetch(id: id))
        } catch .notFound(let missing) {
            out.append("missing {missing}")
        }
    }
    return out
}
