// `* / %` bind tighter than `+ -`, both are left-associative, prefix `-` binds tightest.
func main() -> List<Int> {
    return [2 + 3 * 4, (2 + 3) * 4, 10 - 4 - 3, 100 / 10 / 5, -2 * 3, 7 - -2, 2 * 3 % 4]
}
