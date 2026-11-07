// A `catch` clause can list several cases.
enum Problem implements Error {
    case a
    case b
    case c
}

func fail(with problem: Problem) throws Problem -> String {
    throw problem
}

func handle(problem: Problem) -> String {
    do {
        return try fail(with: problem)
    } catch .a, .b {
        return "a or b"
    } catch .c {
        return "c"
    }
}

func main() -> List<String> {
    return [handle(problem: .a), handle(problem: .b), handle(problem: .c)]
}
