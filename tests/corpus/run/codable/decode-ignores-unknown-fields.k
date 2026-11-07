// Decoding ignores fields the type doesn't have.
import k.encoding.Json
import k.http.DecodingError

struct Point implements Codable {
    let x: Int
    let y: Int
}

func main() throws DecodingError -> Point {
    return try Json.decode("{{\"x\": 1, \"y\": 2, \"z\": 3}")
}
