// `/` truncates toward zero for every sign combination, and never yields -0.
func main() -> List<Int> {
    return [-7 / -2, -1 / 2, 1 / -2, 0 / 5, 9 / 3, -9 / 3]
}
