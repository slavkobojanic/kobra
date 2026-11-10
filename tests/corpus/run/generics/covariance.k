// Read-only positions are covariant: a list of structs is a list of an
// interface they implement (each element gets its adapter), and so is an
// optional list; a map's values and a result's value convert where the
// representation doesn't change.
interface Priced {
    func price() -> Int
}

struct Card implements Priced {
    let cents: Int

    func price() -> Int {
        return cents
    }
}

class Coupon implements Priced {
    func price() -> Int {
        return -5
    }
}

func total(_ items: List<Priced>) -> Int {
    var sum = 0
    for item in items {
        sum += item.price()
    }
    return sum
}

func totalOf(_ items: List<Priced>?) -> Int {
    guard let all = items else { return 0 }
    return total(all)
}

func cheapest(_ byName: Map<String, Priced>) -> Int {
    var low = 1000
    for (_, item) in byName {
        if item.price() < low {
            low = item.price()
        }
    }
    return low
}

func main() -> List<Int> {
    let cards = [Card(cents: 10), Card(cents: 32)]
    let coupons: Map<String, Coupon> = ["a": Coupon()]
    let maybe: List<Card>? = cards
    let found: Result<Coupon, Never> = .success(Coupon())
    let wide: Result<Priced, Never> = found
    let value = match wide {
        case .success(let p) { p.price() }
        case .failure { 0 }
    }
    return [total(cards), totalOf(cards), totalOf(maybe), cheapest(coupons), value]
}
