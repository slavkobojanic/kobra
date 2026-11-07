// `await` covers the whole expression to its right (grammar.md, "try and
// await"): `await next() == 2` awaits the call, then compares its value,
// it doesn't compare the pending call. Short-circuiting still holds, and a
// member or an element of an awaited call (`await point()?.x`,
// `await items()[1]`) reads the call's value.
import k.observe.Log

func next(_ n: Int) async -> Int {
    Log.info("next {n}")
    return n
}

struct Point {
    let x: Int
}

func point() async -> Point? {
    return Point(x: 7)
}

func items() async -> List<Int> {
    return [4, 5]
}

func main() async -> List<Bool> {
    let equal = await next(2) == 2
    let sum = await next(1) + next(2) == 3
    let shortCircuit = await next(1) == 0 and next(9) == 9
    let negated = not (await next(3) == 4)
    let member = await point()?.x == 7
    let x = await point()?.x
    let element = await items()[1] == 5
    return [equal, sum, shortCircuit, negated, member, x == 7, element]
}
