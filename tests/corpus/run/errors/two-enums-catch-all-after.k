// A clause naming one enum's case doesn't take the other enum's case at the
// same position; a catch-all after the named clauses takes the rest.
enum DeviceError implements Error {
    case unsupported
    case cancelled
}

enum SignError implements Error {
    case taken
    case notAccepted
}

func device(n: Int) throws DeviceError -> Int {
    if n == 1 {
        throw DeviceError.cancelled
    }
    return n
}

func server(n: Int) throws SignError -> Int {
    if n == 3 {
        throw SignError.notAccepted
    }
    return n
}

func run(n: Int) -> String {
    do {
        let d = try device(n: n)
        return "ok {try server(n: d)}"
    } catch .notAccepted {
        return "not accepted"
    } catch _ {
        return "something else"
    }
}

func main() -> List<String> {
    return [run(n: 1), run(n: 3), run(n: 4)]
}
