// A use of a `@Deprecated` function is a K1401 warning. Its replacement has
// the same labels, so the warning has a fix to call `total(of:)`; the
// declaration itself and its doc link aren't uses.

@Deprecated("use `total(of:)`", since: "0.1", replacement: "total(of:)")
func sum(of values: List<Int>) -> Int {
    return sum(of: values)
}

func total(of values: List<Int>) -> Int {
    return 0
}

/// Adds up with [`sum`], for now.
func report() -> Int {
    return sum(of: [1, 2]) + sum(of: [3])
}
