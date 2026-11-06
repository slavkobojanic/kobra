// `else` on the next line continues a `try` (rule N3)
func f() {
    let v = try load()
        else { nil }
}
