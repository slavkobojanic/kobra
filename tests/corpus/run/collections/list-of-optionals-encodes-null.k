// A list of optionals encodes nil as null.
func main() -> List<String?> {
    return ["a", nil, "c"]
}
