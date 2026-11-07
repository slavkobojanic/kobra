// Malformed text and a wrong shape are typed `DecodingError` cases, not crashes.
import k.encoding.Json
import k.http.DecodingError

struct Point implements Codable {
    let x: Int
    let y: Int
}

func attempt(_ text: String) -> String {
    do {
        let p: Point = try Json.decode(text)
        return "ok {p.x}"
    } catch .malformed {
        return "malformed"
    } catch .mismatch {
        return "mismatch"
    }
}

func main() -> List<String> {
    return [attempt("{{\"x\": 1, \"y\": 2}"), attempt("{{\"x\": 1"), attempt("{{\"x\": \"one\", \"y\": 2}"), attempt("{{\"x\": 1}"), attempt("[]"), attempt(""), attempt("{{\"x\": 1.5, \"y\": 2}")]
}
