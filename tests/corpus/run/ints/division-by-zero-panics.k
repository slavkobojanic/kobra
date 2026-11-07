// Integer division by zero panics in every build.
func divide(a: Int, b: Int) -> Int {
    return a / b
}

func main() -> Int {
    return divide(a: 1, b: 0)
}
