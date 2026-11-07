// Currency codes, minor-unit digits and parsing by code.
func main() -> List<String> {
    let parsed = Currency.parse(code: "EUR")
    let unknown = Currency.parse(code: "XYZ")
    let lower = Currency.parse(code: "usd")
    return [Currency.usd.code, "{Currency.jpy.minorUnitDigits}", "{Currency.usd.minorUnitDigits}", "{parsed == .eur}", "{unknown == nil}", "{lower == nil}", 5.chf.currency.code]
}
