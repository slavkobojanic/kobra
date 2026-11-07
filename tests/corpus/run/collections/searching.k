// `firstWhere` and `indexWhere` find the first match, or nil.
func main() -> List<Int?> {
    let values = [5, 8, 12, 8]
    return [values.firstWhere { v => v > 6 }, values.indexWhere { v => v == 8 }, values.firstWhere { v => v > 100 }, values.indexWhere { v => v < 0 }]
}
