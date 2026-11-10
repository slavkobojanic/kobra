// An `Error` is thrown as it is, so every catch upstream sees what it was
// given: a catch-all rebinds the same value (also where the `do` throws an
// `Error` and a concrete type), and `try … else`, `Result.of` and a
// generic error parameter pass it on. A log line and the uncaught error of
// `main` show the case, never the value's wrapping.
import k.observe.Log

enum A implements Error, Codable {
    case x
    case y(Int)
}

enum B implements Error {
    case z
}

func fails(n: Int) throws A -> Int {
    if n == 1 {
        throw A.x
    }
    if n == 2 {
        throw A.y(7)
    }
    return n
}

func other(n: Int) throws B -> Int {
    if n == 3 {
        throw B.z
    }
    return n
}

func either(n: Int) throws Error -> Int {
    do {
        return try other(n: try fails(n: n))
    } catch let error {
        throw error
    }
}

func mixed(n: Int) throws Error -> Int {
    do {
        if n == 5 {
            throw A.y(5)
        }
        return try either(n: n)
    } catch A.y(let m) where m == 5 {
        return 50
    } catch let error {
        throw error
    }
}

func caught(n: Int) -> String {
    do {
        return "ok {try mixed(n: n)}"
    } catch let error {
        Log.error("failed: {error}")
        let errors: List<Error> = [error]
        return "failed {error} {errors}"
    }
}

func results(n: Int) -> String {
    let r: Result<Int, Error> = Result.of { try mixed(n: n) }
    let fallback = try mixed(n: n) else { -1 }
    return match r {
        case .success(let v) { "success {v} {fallback}" }
        case .failure(let e) { "failure {e} {fallback}" }
    }
}

func twice<T, E>(_ block: () throws E -> T) throws E -> List<T> {
    let first = try block()
    let second = try block()
    return [first, second]
}

func attempt<T>(_ block: () throws Error -> T) -> Result<T, Error> {
    return Result.of { try block() }
}

func outcome(_ r: Result<Int, Error>) -> String {
    return match r {
        case .success(let v) { "{v}" }
        case .failure(let e) { "{e}" }
    }
}

func main() throws Error -> Int {
    for n in [0, 1, 2, 3, 5] {
        Log.info(caught(n: n))
    }
    for n in [0, 2, 3] {
        Log.info(results(n: n))
    }
    let pair = try twice { try mixed(n: 4) }
    let failed = try twice { try mixed(n: 3) } else { [-1] }
    Log.info("{pair} {failed} {outcome(attempt { try mixed(n: 1) })} {outcome(attempt { 8 })}")
    return try mixed(n: 2)
}
