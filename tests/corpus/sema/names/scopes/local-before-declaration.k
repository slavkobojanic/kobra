// A local is not visible before its declaration: an earlier read is K1201.
func area(width: Int) -> Int {
    let total = width * height
    let height = 2
    return total + height
}
