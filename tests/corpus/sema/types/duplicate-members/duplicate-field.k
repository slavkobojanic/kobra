// Two stored fields named `a` are K2019 at the second, with a label at the
// first. The memberwise initializer keeps the first, so `S(a: 1)` is fine:
// no K2008 claiming `a` is missing.
struct S {
    let a: Int
    let a: Int
}

let s = S(a: 1)
