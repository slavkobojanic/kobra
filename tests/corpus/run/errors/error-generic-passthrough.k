// A function taking `() throws E -> T` throws what the block throws, and nothing when the block can't throw.
enum Oops implements Error {
    case oops
}

func twice<T, E>(_ block: () throws E -> T) throws E -> List<T> {
    let first = try block()
    let second = try block()
    return [first, second]
}

func fails() throws Oops -> Int {
    throw Oops.oops
}

func main() -> List<Int> {
    var n = 0
    let counted = twice {
        n += 1
        n
    }
    let failed = try twice { try fails() } else { [-1] }
    return counted + failed + [n]
}
