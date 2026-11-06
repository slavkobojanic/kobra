// `try x else { ... }` attaches the else block to the TryExpr
func f() {
    let item = try cart.add(p) else { return }
    let n = try parse(s) else { 0 }
}
