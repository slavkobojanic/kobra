// `try await … else` falls back on an async error.
enum Gone implements Error {
    case gone
}

func read(key: String) async throws Gone -> String {
    if key == "a" {
        return "alpha"
    }
    throw Gone.gone
}

func main() async -> List<String> {
    let a = try await read(key: "a") else { "?" }
    let b = try await read(key: "b") else { "?" }
    return [a, b]
}
