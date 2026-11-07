// A panic inside a lambda passed to a stdlib function ends the program.
func main() -> List<Int> {
    let divisors = [1, 2, 0, 4]
    return divisors.map { d => 12 / d }
}
