// A `Map<String, List<Int>>` encodes as a JSON object.
func main() -> Map<String, List<Int>> {
    return [1, 2, 3, 4, 5].groupedBy { n => if n % 2 == 0 { "even" } else { "odd" } }
}
