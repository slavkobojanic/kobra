// `sum()` on lists of Int and Double, including negative and empty lists.
func main() -> List<String> {
    let none: List<Double> = []
    return ["{[1, -2, 3].sum()}", "{[0.5, 0.25].sum()}", "{none.sum()}", "{[0.1, 0.2].sum()}"]
}
