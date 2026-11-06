// `self` exists only in the body of a type or an extension: in a top-level function it is K1201.
func current() -> Int {
    return self
}
