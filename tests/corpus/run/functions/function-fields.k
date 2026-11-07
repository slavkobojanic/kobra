// A struct can hold a function and call it.
struct Rule {
    let name: String
    let check: (Int) -> Bool
}

func main() -> List<String> {
    let rules = [Rule(name: "positive", check: { n => n > 0 }), Rule(name: "small", check: { n => n < 10 })]
    return rules.filter { rule => rule.check(42) }.map { rule => rule.name }
}
