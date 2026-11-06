// a `do` block needs at least one `catch`: K0100 at the gap after the block, and the next statement still parses
func f() {
    do {
        work()
    }
    let a = 1
}
