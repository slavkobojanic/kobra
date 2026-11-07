// Fuzz regression (stage 3 `differential`, codable round trip): `Json.encode` drops the entry of a
// `Map<String, _>` whose key is `__proto__`, because the map codec assigns entries to a plain `{}`
// object, where `o["__proto__"] = v` sets the prototype instead of a property. Decoding keeps it,
// so a round trip loses data a client sent.
import k.encoding.Json
import k.http.DecodingError

func main() throws DecodingError -> List<String> {
    let m: Map<String, Bool> = ["__proto__": true, "a": false]
    let back: Map<String, Bool> = try Json.decode("{{\"__proto__\": true}")
    return [Json.encode(m), "{back.count}", Json.encode(back)]
}
