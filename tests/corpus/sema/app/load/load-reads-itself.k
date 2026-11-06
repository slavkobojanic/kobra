// A load that reads its own field is a cycle of one.
func next(value: Int) async -> Int {
    return value + 1
}

page Loop {
    @Load let a: Int = await next(value: a)

    view {
        text { "{a}" }
    }

    loading {
    }
}
