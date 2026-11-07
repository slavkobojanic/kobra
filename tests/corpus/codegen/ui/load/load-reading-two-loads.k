// A load reading two others waits for both; `after` lists them in declaration order.
service Numbers {
    func one() async -> Int {
        return 1
    }

    func two() async -> Int {
        return 2
    }

    func sum(a: Int, b: Int) async -> Int {
        return a + b
    }
}

page Sum {
    @Inject let numbers: Numbers

    @Load let a: Int = await numbers.one()
    @Load let b: Int = await numbers.two()
    @Load let total: Int = await numbers.sum(a: a, b: b)

    view {
        text { "{total}" }
    }

    loading {
    }
}
