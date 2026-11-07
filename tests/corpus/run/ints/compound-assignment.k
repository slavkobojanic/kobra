// `+=`, `-=`, `*=`, `/=` and `%=` update a `var` in place.
func main() -> List<Int> {
    var n = 10
    n += 5
    n -= 3
    n *= 2
    var q = 17
    q /= 5
    var r = 17
    r %= 5
    return [n, q, r]
}
