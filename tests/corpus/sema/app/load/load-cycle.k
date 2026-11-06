// `@Load` initializers that read each other form a cycle; every field in it is reported.
func next(value: Int) async -> Int {
    return value + 1
}

page Loop {
    @Load let a: Int = await next(value: b)
    @Load let b: Int = await next(value: a)

    view {
        text { "{a} {b}" }
    }

    loading {
    }
}
