// `throw` of an unresolved name is K1201 only, no K2203.
func check(ready: Bool) {
    if ready {
        throw Problem.failed
    }
}
