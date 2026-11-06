// `catch` at the start of a line continues the `do` (rule N3)
func f() {
    do {
        try work()
    }
    catch .a {
    }
    catch .b {
    }
}
