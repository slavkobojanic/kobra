// `where T implements X` accepts arguments whose type implements `X`; members of `X` are usable on `T`.
interface Priced {
    var price: Int
}

struct Mug implements Priced {
    let price: Int
}

func cheapest<T>(items: List<T>) -> T? where T implements Priced {
    return items.minBy { item => item.price }
                                    //^^^^^ resolves: member corpus.Priced.price
}

let mug = cheapest(items: [Mug(price: 3), Mug(price: 1)])
  //^^^ type: corpus.Mug?
