// Using it as a type in another file is K1301; `line.quantity` reports nothing more.
func count(line: Line) -> Int {
    return line.quantity
}
