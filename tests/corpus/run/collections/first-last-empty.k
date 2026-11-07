// `first`, `last`, `isEmpty` and `isNotEmpty` on empty and non-empty lists.
func main() -> List<String> {
    let values = [4, 5, 6]
    let none: List<Int> = []
    return ["{values.first}", "{values.last}", "{none.first}", "{none.last}", "{values.isEmpty}", "{none.isEmpty}", "{none.isNotEmpty}"]
}
