// Fuzz regression (stage 3 `differential`, codable mutations): a JSON array decodes as an empty
// `Map<String, _>` instead of a `mismatch`, because the map codec only checks `typeof j === "object"`
// (codegen.md §5.1: decoding validates types).
import k.encoding.Json
import k.http.DecodingError

func attempt(_ text: String) -> String {
    do {
        let m: Map<String, Int> = try Json.decode(text)
        return "ok {m.count}"
    } catch .malformed {
        return "malformed"
    } catch .mismatch {
        return "mismatch"
    }
}

func main() -> List<String> {
    return [attempt("{{\"a\": 1}"), attempt("[]"), attempt("[1, 2]"), attempt("null")]
}
