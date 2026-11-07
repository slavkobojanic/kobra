// String values are escaped in JSON text.
import k.encoding.Json

func main() -> String {
    return Json.encode(["a\"b", "line\nbreak", "back\\slash"])
}
