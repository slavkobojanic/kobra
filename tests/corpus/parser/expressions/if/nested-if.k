// ifs nested inside branches
func f() {
    if a {
        if b {
            if c { x() } else { y() }
        }
    }
}
