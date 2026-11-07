// `Int.parse` accepts optionally negative decimal digits and nothing else.
func main() -> List<Int?> {
    return [Int.parse("42"), Int.parse("-7"), Int.parse(""), Int.parse("abc"), Int.parse("4.5"), Int.parse("12abc"), Int.parse("0")]
}
