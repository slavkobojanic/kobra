// A generic function's type parameter is inferred at each call site.
func firstOr<T>(items: List<T>, fallback: T) -> T {
    return items.first ?? fallback
}

let n = firstOr(items: [1, 2], fallback: 0)
  //^ type: Int
let s = firstOr(items: ["a"], fallback: "")
  //^ type: String
