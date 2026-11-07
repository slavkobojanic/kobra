// A `var` optional can be set to nil and back.
func main() -> List<String?> {
    var name: String? = "Ana"
    let first = name
    name = nil
    let second = name
    name = "Ben"
    return [first, second, name]
}
