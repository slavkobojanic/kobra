// An async `main` that throws reports the error as its result.
enum Down implements Error, Codable {
    case unavailable(service: String)
}

func ping() async throws Down {
    throw Down.unavailable(service: "db")
}

func main() async throws Down -> String {
    try await ping()
    return "unreachable"
}
