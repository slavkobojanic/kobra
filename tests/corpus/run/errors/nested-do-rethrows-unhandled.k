// An inner `do` that handles only some cases lets the rest reach the outer `do`.
enum NetError implements Error {
    case timeout
    case refused
}

func call(n: Int) throws NetError -> Int {
    if n == 0 {
        throw NetError.timeout
    }
    if n == 1 {
        throw NetError.refused
    }
    return n
}

func run(n: Int) -> String {
    do {
        do {
            return "value {try call(n: n)}"
        } catch .timeout {
            return "inner timeout"
        }
    } catch .refused {
        return "outer refused"
    }
}

func main() -> List<String> {
    return [run(n: 0), run(n: 1), run(n: 2)]
}
