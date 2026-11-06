// `guard let value` unwraps the parameter `value` into a local of the same name for the rest of the body.
func length(of value: String?) -> Int {
    guard let value else {
        return 0
    }
    return value.count
    //     ^^^^^ resolves: local value 3:15
}
