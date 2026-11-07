// A Result of Codable parts encodes like any Codable enum.
enum Bad implements Error, Codable {
    case tooBig(limit: Int)
}

func main() -> List<Result<Int, Bad>> {
    return [.success(3), .failure(.tooBig(limit: 10))]
}
