// Money decodes back exactly, without floating-point rounding, in currencies with different digits.
import k.encoding.Json
import k.http.DecodingError

struct Price implements Codable, Equatable {
    let amount: Money
}

func main() throws DecodingError -> List<Bool> {
    let prices = [Price(amount: 0.10.usd), Price(amount: 1500.jpy), Price(amount: 19.99.eur)]
    var same: List<Bool> = []
    for price in prices {
        let back: Price = try Json.decode(Json.encode(price))
        same.append(back == price)
    }
    return same
}
