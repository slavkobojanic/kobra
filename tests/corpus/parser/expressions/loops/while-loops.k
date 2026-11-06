// `while` takes a condition list, including `let` conditions
func f() {
    while count > 0 { count -= 1 }
    while let next = it.next() { use(next) }
}
