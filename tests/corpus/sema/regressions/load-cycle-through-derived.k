// `@Load` ordering follows reactive reads (14-async-data.md): `a` reads the
// `@Derived` `d`, which reads `b`, which reads `a`. That is a load cycle (K3020 at
// each `@Load` field in it) even though no load names another load directly.
func next(value: Int) async -> Int {
    return value + 1
}

page Loop {
    @Load let a: Int = await next(value: d)
    @Derived let d: Int = b * 2
    @Load let b: Int = await next(value: a)

    view {
        text { "{a} {b}" }
    }

    loading {
    }
}
