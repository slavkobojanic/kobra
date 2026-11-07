// The `else` block of `try … else` may leave the function instead of producing a value.
enum Bad implements Error {
    case bad
}

func parse(text: String) throws Bad -> Int {
    guard let n = Int.parse(text) else {
        throw Bad.bad
    }
    return n
}

func doubled(text: String) -> String {
    let n = try parse(text: text) else {
        return "not a number"
    }
    return "{n * 2}"
}

func main() -> List<String> {
    return [doubled(text: "21"), doubled(text: "x")]
}
