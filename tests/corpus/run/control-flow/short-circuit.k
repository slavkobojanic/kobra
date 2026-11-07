// `and` and `or` evaluate their right side only when needed.
import k.observe.Log

func check(_ name: String, _ value: Bool) -> Bool {
    Log.info(name)
    return value
}

func main() -> List<Bool> {
    let a = check("a", false) and check("b", true)
    let c = check("c", true) or check("d", true)
    let e = check("e", true) and check("f", false)
    let g = not check("g", false) or check("h", true)
    return [a, c, e, g]
}
