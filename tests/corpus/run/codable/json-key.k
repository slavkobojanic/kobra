// `@JsonKey` names the JSON key of a field in both directions.
import k.encoding.Json
import k.http.DecodingError

struct Reading implements Codable {
    @JsonKey("temperature_2m") let temperature: Double
    let unit: String
}

func main() throws DecodingError -> List<String> {
    let r: Reading = try Json.decode("{{\"temperature_2m\": 21.5, \"unit\": \"C\"}")
    return ["{r.temperature}", Json.encode(r)]
}
