// Typed throws: `catch` clauses match cases and bind payloads; `catch let` takes the rest.
enum ParseError implements Error {
    case empty
    case tooLong(length: Int)
    case invalid
}

func parse(text: String) throws ParseError -> Int {
    if text.isEmpty {
        throw ParseError.empty
    }
    if text.count > 3 {
        throw ParseError.tooLong(length: text.count)
    }
    guard let n = Int.parse(text) else {
        throw ParseError.invalid
    }
    return n
}

func attempt(text: String) -> String {
    do {
        let n = try parse(text: text)
        return "ok {n}"
    } catch .empty {
        return "empty"
    } catch .tooLong(let length) {
        return "too long ({length})"
    } catch let other {
        return "other: {other}"
    }
}

func main() -> List<String> {
    return [attempt(text: "42"), attempt(text: ""), attempt(text: "12345"), attempt(text: "ab")]
}
