// A catch-all over two error enums binds an `Error`: its cases can't be
// matched, since an enum value doesn't carry its enum (K2312 at each case).
enum DeviceError implements Error {
    case unsupported
    case cancelled
}

enum SignError implements Error {
    case taken
    case notAccepted
}

func device() throws DeviceError -> Int {
    return 1
}

func server(n: Int) throws SignError -> Int {
    return n
}

func run() -> String {
    do {
        return "ok {try server(n: try device())}"
    } catch let problem {
        match problem {
            case DeviceError.cancelled { return "cancelled" }
            case SignError.notAccepted { return "not accepted" }
            default { return "other" }
        }
    }
}
