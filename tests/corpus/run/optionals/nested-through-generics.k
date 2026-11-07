// `first` of a `List<Int?>` keeps "empty list" and "first element is nil" apart.
func describe(values: List<Int?>) -> String {
    if let first = values.first {
        return "first is {first}"
    }
    return "empty"
}

func main() -> List<String> {
    let none: List<Int?> = []
    return [describe(values: [nil, 1]), describe(values: [2]), describe(values: none)]
}
