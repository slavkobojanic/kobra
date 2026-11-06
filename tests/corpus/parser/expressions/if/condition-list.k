// conditions combine with commas: `if let a, let b = c, a.isReady { }`
func f() {
    if let a, let b = c, a.isReady, not b.isEmpty { }
}
