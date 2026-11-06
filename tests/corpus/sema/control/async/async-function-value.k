// `async` is part of a function type: a function value keeps it, and calling
// the value needs `await` like calling the function.
func fetchCount() async -> Int {
    return 3
}

func run() async -> Int {
    let fetch = fetchCount
//      ^^^^^ type: () async -> Int
    let first = await fetch()
    return first + fetch()
}
