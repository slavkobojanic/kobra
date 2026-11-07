// A generic function constrained to Codable encodes and decodes the concrete type it is called with.
import k.encoding.Json
import k.http.DecodingError

struct Point implements Codable {
    let x: Int
    let y: Int
}

func roundTrip<T>(_ value: T) throws DecodingError -> T where T implements Codable {
    return try Json.decode(Json.encode(value))
}

func main() throws DecodingError -> List<String> {
    let p = try roundTrip(Point(x: 1, y: 2))
    let words = try roundTrip(["a", "b"])
    return ["{p.x},{p.y}"] + words
}
