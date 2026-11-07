// Recursion and default parameters.
func fibonacci(n: Int) -> Int {
    if n < 2 {
        return n
    }
    return fibonacci(n: n - 1) + fibonacci(n: n - 2)
}

func greet(name: String, greeting: String = "Hello") -> String {
    return "{greeting}, {name}"
}

func main() -> List<String> {
    return ["{fibonacci(n: 20)}", greet(name: "Ana"), greet(name: "Ben", greeting: "Hi")]
}
