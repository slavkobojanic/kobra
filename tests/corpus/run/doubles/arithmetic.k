// Double arithmetic is IEEE 754: no rounding to decimal, `%` keeps the fraction.
func main() -> List<Double> {
    return [0.1 + 0.2, 1.5 * 2.0, 7.0 / 2.0, 7.5 % 2.0, -0.5 - 0.25]
}
