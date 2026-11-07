// Map literals, subscripts, `set`, `get`, `remove`, `containsKey` and `count`.
func main() -> List<String> {
    var stock: Map<String, Int> = ["mug": 3, "cup": 0]
    stock["plate"] = 7
    stock.set("cup", 2)
    stock.remove("mug")
    stock.remove("missing")
    return ["{stock.count}", "{stock["cup"]}", "{stock.get("plate")}", "{stock["mug"]}", "{stock.containsKey("mug")}", "{stock.containsKey("plate")}"]
}
