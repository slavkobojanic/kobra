// Double equality is exact, so 0.1 + 0.2 is not 0.3.
func main() -> List<Bool> {
    let sum = 0.1 + 0.2
    return [sum == 0.3, 0.5 < 0.75, 2.0 >= 2.0, sum != 0.3]
}
