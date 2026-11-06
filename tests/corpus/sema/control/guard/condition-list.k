// A condition list binds in order; later conditions use earlier bindings, and
// all bindings stay in scope after the `guard`.
func sum(a: Int?, b: Int?) -> Int {
    guard let x = a, let y = b, x < y else {
        return 0
    }
    return x + y
//             ^ resolves: local y 4:26
}
