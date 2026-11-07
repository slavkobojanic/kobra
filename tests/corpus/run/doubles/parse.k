// `Double.parse` accepts decimal numbers, whole ones included.
func main() -> List<Double?> {
    return [Double.parse("3.25"), Double.parse("1"), Double.parse("-0.5"), Double.parse("abc"), Double.parse("")]
}
