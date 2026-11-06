// An `if`/`else` used as the lambda's last expression gives the lambda its value type.
func signs(numbers: List<Int>) -> List<String> {
    return numbers.map { n =>
        if n < 0 { "minus" } else { "plus" }
    }
}
//@ 4:9-4:45 type: String
