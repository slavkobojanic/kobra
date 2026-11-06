// A top-level `let` can't be assigned; it is not local, so there is no `var` fix (top-level `var` is K0131).
let limit = 10

func raise() {
    limit = 20
}
