// `catch unknown` catches the cases its clauses don't name, like `catch _`.
enum Fault implements Error {
    case one
    case two
}

func risky(n: Int) throws Fault -> Int {
    if n == 1 {
        throw Fault.one
    }
    if n == 2 {
        throw Fault.two
    }
    return n
}

func safe(n: Int) -> Int {
    do {
        return try risky(n: n)
    } catch .one {
        return -1
    } catch unknown {
        return -99
    }
}

func main() -> List<Int> {
    return [safe(n: 0), safe(n: 1), safe(n: 2)]
}
