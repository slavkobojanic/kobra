// payloads bind with `let`, match by label, nest, and may be empty
func f() {
    match r {
        case .outOfStock(available: let n) { }
        case .loaded(.some(let p), count: 0) { }
        case .empty() { }
        case .pair(let a, let b,) { }
    }
}
