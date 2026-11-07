// Top-level functions can call each other before their declaration.
func isEven(n: Int) -> Bool {
    if n == 0 {
        return true
    }
    return isOdd(n: n - 1)
}

func isOdd(n: Int) -> Bool {
    if n == 0 {
        return false
    }
    return isEven(n: n - 1)
}

func main() -> List<Bool> {
    return [isEven(n: 10), isOdd(n: 7), isEven(n: 3)]
}
