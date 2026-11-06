// A call returning `Never` doesn't reach the next statement (sema.md §11.2 counts
// it as an exit for `guard`), so the branch that calls it doesn't leave `x`
// unassigned: no K2306.
func fail() -> Never {
    return fail()
}

func f(b: Bool) -> Int {
    let x: Int
    if b {
        x = 1
    } else {
        fail()
    }
    return x
}
