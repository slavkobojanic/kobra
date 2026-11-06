// `break` and `continue` are statements of their own
func f() {
    for row in rows {
        for cell in row {
            if cell.isEmpty { continue }
            if cell.isLast { break }
        }
    }
}
