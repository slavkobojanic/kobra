// An `Error` (a catch-all over two error types) interpolates as its case
// name, as a value of its own enum does: payloads aren't printed. That
// holds for stdlib errors, and for an `Error` passed on, stored or listed.
import k.encoding.Json
import k.http.DecodingError

enum DeviceError implements Error {
    case unsupported
    case denied(reason: String)
}

enum SignError implements Error {
    case taken
    case limited(Int)
}

struct Point implements Codable {
    let x: Int
}

func device(n: Int) throws DeviceError -> Int {
    if n == 1 {
        throw DeviceError.unsupported
    }
    if n == 2 {
        throw DeviceError.denied(reason: "no camera")
    }
    return n
}

func sign(n: Int) throws SignError -> Int {
    if n == 3 {
        throw SignError.taken
    }
    if n == 4 {
        throw SignError.limited(7)
    }
    return n
}

func shown(_ error: Error) -> String {
    return "<{error}>"
}

func attempt(n: Int) -> String {
    do {
        let d = try device(n: n)
        return "ok {try sign(n: d)}"
    } catch let error {
        return "failed: {error} {shown(error)}"
    }
}

func decoded(_ text: String, n: Int) -> String {
    do {
        let p: Point = try Json.decode(text)
        return "ok {try device(n: p.x + n)}"
    } catch let error {
        let kept: Error? = error
        let all: List<Error> = [error, error]
        return "{error} {kept} {all}"
    }
}

func passedOn() -> String {
    let none: Error? = nil
    return "{shown(SignError.limited(3))} {shown(DecodingError.malformed(message: "x"))} {none}"
}

func main() -> List<String> {
    return [
        "{DeviceError.denied(reason: "x")} {SignError.limited(1)}",
        attempt(n: 1), attempt(n: 2), attempt(n: 3), attempt(n: 4), attempt(n: 5),
        decoded("{{\"x\": 1", n: 0), decoded("{{\"x\": \"one\"}", n: 0), decoded("{{\"x\": 1}", n: 1),
        passedOn(),
    ]
}
