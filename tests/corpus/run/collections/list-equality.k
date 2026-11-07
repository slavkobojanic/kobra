// Lists are equal when they have equal elements in the same order.
func main() -> List<Bool> {
    let none: List<Int> = []
    return [[1, 2] == [1, 2], [1, 2] == [2, 1], [[1], [2]] == [[1], [2]], none == [], [1] != [1, 1]]
}
