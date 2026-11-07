// Interpolation holds any expression: arithmetic, members, calls and comparisons; adjacent parts concatenate.
func main() -> List<String> {
    let a = 2
    let b = 3
    let name = "ana"
    return ["{a + b}", "{a * b} items", "{[1, 2].count}", "{name.uppercased()}", "{a > b}", "{a}{b}", "{"x" + name}"]
}
