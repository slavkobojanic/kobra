// A component's async function passed to a K function's parameter is called the way K calls a
// function value, with the caller's context first, so it is passed as it is, not bound to the
// component with its K arguments alone (`$k.own`), which would shift its arguments by one
// (codegen.md §8, §10).
func each(_ f: (Int) async -> Void) async {
    await f(5)
}

component Editor {
    @State var last: Int = 0

    func show(id: Int) async {
        last = id
    }

    func run() async {
        await each(show)
    }

    view {
        button { "Run" }
            .on(click: run)
    }
}
