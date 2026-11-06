// Calls of a function whose parameter type is unresolved report nothing at the call site.
func area(shape: Shap) -> Int {
    return 0
}

let size = area(shape: 3)
