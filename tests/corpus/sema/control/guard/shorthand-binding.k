// `guard let value` is short for `guard let value = value`; the unwrapped
// binding shadows the parameter after the `guard`.
func double(value: Int?) -> Int {
    guard let value else {
        return 0
    }
    return value * 2
//         ^^^^^ type: Int
}
