// Every way a concrete error becomes an `Error` keeps its text: a returned
// value (an explicit `return`, a getter's value, an async function's), the
// branches of an `if` that is a lambda's value, a field's default, a
// default argument, an optional, and a generic function instantiated at
// `Error`. A returned struct becomes a project interface's value the same
// way.
enum A implements Error {
    case x
    case y(Int)
}

enum B implements Error {
    case z
}

interface Shape {
    func area() -> Int
}

struct Square implements Shape {
    let side: Int

    func area() -> Int {
        return side * side
    }
}

struct Holder {
    let error: Error = A.y(3)
    let other: Error? = B.z

    var latest: Error {
        A.y(4)
    }
}

func picked(n: Int) -> Error {
    if n > 0 {
        return A.y(n)
    }
    return B.z
}

func maybe(n: Int) -> Error? {
    if n > 0 {
        return A.x
    }
    return nil
}

func later() async -> Error {
    return A.y(9)
}

func shape() -> Shape {
    return Square(side: 3)
}

func shown(_ error: Error = B.z) -> String {
    return "<{error}>"
}

func same<E>(_ e: E) -> E where E implements Error {
    return e
}

func widened<E>(_ e: E) -> Error where E implements Error {
    return e
}

func viaLambda(n: Int) -> String {
    let f: (Int) -> Error = { k =>
        if k > 0 {
            A.y(k)
        } else {
            B.z
        }
    }
    return "{f(n)}"
}

func main() async -> List<String> {
    let h = Holder()
    let given: Error = same(A.y(1))
    let kept: Error = same(given)
    let fromGeneric = widened(B.z)
    let passedOn = widened(given)
    return [
        "{picked(n: 1)} {picked(n: 0)} {maybe(n: 1)} {maybe(n: 0)}",
        "{await later()} {shape().area()}",
        "{h.error} {h.other} {h.latest} {shown()} {shown(A.x)}",
        "{viaLambda(n: 2)} {viaLambda(n: 0)}",
        "{given} {kept} {fromGeneric} {passedOn}",
    ]
}
