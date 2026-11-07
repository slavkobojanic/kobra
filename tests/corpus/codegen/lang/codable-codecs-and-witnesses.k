// A Codable type gets a codec pair, and a generic function constrained to Codable receives a
// witness `$w_T` that the call site fills (docs/compiler/codegen.md §5.1, §9).
import k.encoding.Json

struct Point implements Codable {
    let x: Int
    let y: Int
}

func text<T>(_ value: T) -> String where T implements Codable {
    return Json.encode(value)
}

func pointText() -> String {
    return text(Point(x: 1, y: 2))
}
