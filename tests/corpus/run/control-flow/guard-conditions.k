// `guard` with a plain condition exits early; code after it sees the condition as true.
func describe(age: Int) -> String {
    guard age >= 0 else {
        return "invalid"
    }
    guard age >= 18 else {
        return "minor"
    }
    return "adult"
}

func main() -> List<String> {
    return [describe(age: -1), describe(age: 5), describe(age: 18)]
}
