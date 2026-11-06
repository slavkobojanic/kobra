// A `match` arm that neither assigns nor exits leaves the binding
// uninitialized after the `match`.
func pick(light: Int) -> Int {
    let b: Int
    match light {
        case 0 { b = 0 }
        case 1 { }
        default { b = 2 }
    }
    return b
}
