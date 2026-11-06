// a `{` on the next line starts a lambda expression statement, not a trailing lambda (rule N4)
func f() {
    let a = f
    { 1 }
}
