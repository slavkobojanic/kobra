// An async function takes the cancellation context `$c` first and every await is guarded by it
// (docs/compiler/codegen.md §8).
func fetch(id: Int) async -> String {
    return "item {id}"
}

func both() async -> List<String> {
    let a = await fetch(id: 1)
    let b = await fetch(id: 2)
    return [a, b]
}
