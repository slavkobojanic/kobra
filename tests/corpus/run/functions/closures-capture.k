// Lambdas capture variables; a function passed by name behaves like a lambda.
func twice(value: Int, using transform: (Int) -> Int) -> Int {
    return transform(transform(value))
}

func increment(n: Int) -> Int {
    return n + 1
}

func main() -> List<Int> {
    let step = 10
    let addStep = { (n: Int) -> Int => n + step }
    return [twice(value: 1, using: addStep), twice(value: 1, using: increment)]
}
