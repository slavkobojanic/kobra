// The right side of `??` runs only when the left side is nil.
import k.observe.Log

func fallback() -> Int {
    Log.info("fallback ran")
    return 0
}

func main() -> List<Int> {
    let some: Int? = 5
    let none: Int? = nil
    return [some ?? fallback(), none ?? fallback()]
}
