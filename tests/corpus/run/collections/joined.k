// `joined` writes each element as text between separators.
func main() -> List<String> {
    let none: List<String> = []
    return [["a", "b", "c"].joined(separator: ", "), [1, 2, 3].joined(separator: "-"), none.joined(separator: ","), ["x"].joined(separator: ",")]
}
