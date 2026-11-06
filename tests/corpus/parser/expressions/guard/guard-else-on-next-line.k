// `else` on the next line continues the guard (rule N3)
func f() {
    guard ready
    else {
        return
    }
}
