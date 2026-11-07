// An optional interpolates as its value, or `nil`.
func main() -> List<String> {
    let some: Int? = 3
    let none: Int? = nil
    let ratio: Double? = 2.0
    let word: String? = "hi"
    return ["{some}", "{none}", "{ratio}", "x{none}y", "{word}"]
}
