// A panic passes through `do`/`catch`, even a catch-all: panics are not errors.
import k.observe.Log

enum Never implements Error {
    case never
}

func boom(n: Int) throws Never -> Int {
    if n < 0 {
        throw Never.never
    }
    return n * 9007199254740991
}

func main() -> Int {
    do {
        return try boom(n: 2)
    } catch let error {
        Log.info("caught {error}")
        return 0
    }
}
