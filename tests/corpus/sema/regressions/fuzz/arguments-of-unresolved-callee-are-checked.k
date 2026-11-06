// Found by the `check_typed` fuzz target (fix property): the arguments of a
// call whose callee doesn't resolve are not checked, so the independent
// unresolved `Later` is only reported once `Missing` is fixed, and applying
// the import fix for `Missing` "adds" an error. Both names are unresolved and
// independent: each must be reported once.
func run() {
    let x = Missing.make(value: Later.one)
    let y = missing(value: later)
}
