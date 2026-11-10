// Payload cases of two error enums at the same positions, caught in one
// `do`: each clause matches its own enum's case only.
enum FetchError implements Error {
    case offline
    case status(code: Int)
}

enum ParseError implements Error {
    case empty
    case badField(name: String)
}

func fetch(n: Int) throws FetchError -> String {
    if n == 0 {
        throw FetchError.status(code: 503)
    }
    if n == 1 {
        throw FetchError.offline
    }
    return "body{n}"
}

func parse(text: String, n: Int) throws ParseError -> Int {
    if n == 2 {
        throw ParseError.badField(name: "id")
    }
    if n == 3 {
        throw ParseError.empty
    }
    return text.count
}

func run(n: Int) -> String {
    do {
        let body = try fetch(n: n)
        return "parsed {try parse(text: body, n: n)}"
    } catch .badField(let name) {
        return "bad field {name}"
    } catch .status(let code) {
        return "status {code}"
    } catch .empty {
        return "empty"
    } catch .offline {
        return "offline"
    }
}

func main() -> List<String> {
    return [run(n: 0), run(n: 1), run(n: 2), run(n: 3), run(n: 4)]
}
