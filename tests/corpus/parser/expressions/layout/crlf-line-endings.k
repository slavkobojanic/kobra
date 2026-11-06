// CRLF line endings terminate statements and allow continuation lines like LF
func f() {
    let names = users
        .filter { u => u.active }
        .map { u => u.name }
    let n = 1 +
        2
}
