// `try … else` on a call that can't throw: K2202 at `try`. Removing `try `
// alone would leave a dangling `else`, so the case pins no fix.
func one() -> Int {
    return 1
}

func show() -> Int {
    return try one() else { 0 }
}
