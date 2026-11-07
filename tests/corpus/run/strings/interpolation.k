// Interpolation formats each part by its type.
import k.observe.Log

func main() {
    let name = "Ana"
    let count = 3
    let ratio = 0.25
    let done = true
    let missing: String? = nil
    Log.info("{name} has {count} items ({ratio}), done: {done}, nickname: {missing}")
    Log.info("braces stay literal: {{ and quotes: \"")
}
