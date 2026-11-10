// Nested `do`s over two error enums: the inner one handles one enum's case,
// and what it leaves, or throws from its handler, reaches the outer one by
// type, with the arguments' `try`s counted before the call's own.
enum DeviceError implements Error {
    case unsupported
    case cancelled
}

enum SignError implements Error {
    case taken
    case notAccepted
}

func device(n: Int) throws DeviceError -> Int {
    if n == 0 {
        throw DeviceError.unsupported
    }
    if n == 1 {
        throw DeviceError.cancelled
    }
    return n
}

func server(n: Int) throws SignError -> Int {
    if n == 3 {
        throw SignError.notAccepted
    }
    if n == 5 {
        throw SignError.taken
    }
    return n
}

func run(n: Int) -> String {
    do {
        do {
            return "ok {try server(n: try device(n: n))}"
        } catch .unsupported {
            return "inner unsupported {try server(n: 5)}"
        }
    } catch .cancelled {
        return "outer cancelled"
    } catch .notAccepted {
        return "outer not accepted"
    } catch .taken {
        return "outer taken"
    }
}

func main() -> List<String> {
    return [run(n: 0), run(n: 1), run(n: 3), run(n: 4)]
}
