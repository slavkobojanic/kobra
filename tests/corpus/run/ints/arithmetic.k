// Int arithmetic: + - * on safe integers, `/` truncates toward zero, `%` has the dividend's sign.
func main() -> List<Int> {
    return [7 + 5, 7 - 12, 6 * 7, 7 / 2, -7 / 2, 7 / -2, 7 % 3, -7 % 3, 0 - 0]
}
