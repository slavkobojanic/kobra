// `Result.of` captures a value or a typed error, and `match` takes it apart.
enum Bad implements Error {
    case negative(Int)
}

func root(n: Int) throws Bad -> Int {
    if n < 0 {
        throw Bad.negative(n)
    }
    var r = 0
    while (r + 1) * (r + 1) <= n {
        r += 1
    }
    return r
}

func describe(n: Int) -> String {
    let result: Result<Int, Bad> = Result.of { try root(n: n) }
    return match result {
        case .success(let v) { "root {v}" }
        case .failure(.negative(let m)) { "negative {m}" }
    }
}

func main() -> List<String> {
    return [describe(n: 17), describe(n: -4), describe(n: 0)]
}
