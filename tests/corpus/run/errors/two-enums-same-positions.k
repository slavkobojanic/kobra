// A `do` whose body throws two error enums matches a clause by the error's
// type and case, not the case's position: `SignError.notAccepted` and
// `DeviceError.cancelled` are both case 1.
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
    if n == 2 {
        throw SignError.taken
    }
    if n == 3 {
        throw SignError.notAccepted
    }
    return n
}

func run(n: Int) -> String {
    do {
        let d = try device(n: n)
        return "ok {try server(n: d)}"
    } catch .cancelled {
        return "cancelled"
    } catch .notAccepted {
        return "not accepted"
    } catch .unsupported {
        return "unsupported"
    } catch .taken {
        return "taken"
    }
}

func main() -> List<String> {
    return [run(n: 0), run(n: 1), run(n: 2), run(n: 3), run(n: 4)]
}
