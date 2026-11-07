// Prefix `-` negates; negating zero is zero.
func main() -> List<Int> {
    let a = 5
    let zero = 0
    return [-a, -(-a), 0 - a, -zero]
}
