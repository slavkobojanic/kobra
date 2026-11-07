// `sum()` on `List<Int>` folds with the overflow check in development-style builds
// (docs/compiler/codegen.md §6.6).
func total(values: List<Int>) -> Int {
    return values.sum()
}

func ratio(values: List<Double>) -> Double {
    return values.sum()
}
