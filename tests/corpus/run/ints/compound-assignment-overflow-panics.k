// Compound assignment is checked like the operator it abbreviates.
func main() -> Int {
    var n = 9007199254740990
    n += 1
    n += 1
    return n
}
