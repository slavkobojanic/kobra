// Functions passed by name behave like lambdas.
func double(n: Int) -> Int {
    return n * 2
}

func isEven(n: Int) -> Bool {
    return n % 2 == 0
}

func main() -> List<Int> {
    return [1, 2, 3, 4].filter(isEven).map(double)
}
