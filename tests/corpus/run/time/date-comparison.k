// Dates are ordered and equal by instant.
func main() -> List<Bool> {
    let a = Date(unixSeconds: 100)
    let b = Date(unixSeconds: 200)
    return [a < b, b > a, a == Date(unixSeconds: 100), a + 100.seconds == b, a != b]
}
