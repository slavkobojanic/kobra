// a line starting with `?.` continues the chain (rule N3)
func f() {
    let city = user
        ?.address
        ?.city
}
