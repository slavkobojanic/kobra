// Functions take and return functions.
func compose(_ f: (Int) -> Int, _ g: (Int) -> Int) -> (Int) -> Int {
    return { x => g(f(x)) }
}

func main() -> List<Int> {
    let addOne = { (x: Int) -> Int => x + 1 }
    let square = { (x: Int) -> Int => x * x }
    let both = compose(addOne, square)
    let other = compose(square, addOne)
    return [both(3), other(3)]
}
