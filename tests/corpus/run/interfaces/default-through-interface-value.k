// A default method called on an interface-typed value uses the conforming type's own members.
interface Priced {
    func price() -> Int

    func priceWithTax() -> Int {
        return price() + price() / 4
    }
}

struct Mug implements Priced {
    func price() -> Int {
        return 8
    }
}

struct Plate implements Priced {
    func price() -> Int {
        return 20
    }

    func priceWithTax() -> Int {
        return 0
    }
}

func checkout(item: Priced) -> Int {
    return item.priceWithTax()
}

func main() -> List<Int> {
    return [checkout(item: Mug()), checkout(item: Plate())]
}
