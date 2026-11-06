// K2205 has a label at the enclosing signature and a maybe-incorrect fix adding
// `async` (diagnostics.md, effects).
func fetch() async {}

func run() -> Int {
    await fetch()
    return 1
}
