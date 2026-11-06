// `bind` outside an argument value is K0110; the BindExpr is kept
func f() {
    let a = bind x
    let b = [bind y]
    let c = f(1 + bind z)
}
