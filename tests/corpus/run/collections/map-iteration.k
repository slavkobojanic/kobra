// `for (key, value) in map` visits entries in insertion order.
func main() -> List<String> {
    let prices: Map<String, Int> = ["tea": 3, "coffee": 4, "water": 1]
    var out: List<String> = []
    for (name, price) in prices {
        out.append("{name}={price}")
    }
    return out
}
