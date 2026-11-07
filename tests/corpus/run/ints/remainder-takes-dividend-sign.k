// `%` has the sign of the dividend; an exact remainder is 0, never -0.
func main() -> List<Int> {
    return [7 % -3, -7 % -3, -6 % 3, 6 % -3, 0 % 7]
}
