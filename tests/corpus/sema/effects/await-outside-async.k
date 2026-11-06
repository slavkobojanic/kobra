func fetch() async -> Int {
    return 1
}

func run() -> Int {
    return await fetch()
}
