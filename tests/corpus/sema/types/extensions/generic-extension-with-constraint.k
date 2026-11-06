// `extend<T> List<T> where T implements X` adds members only to lists whose element implements `X`.
interface Priced {
    var price: Int
}

struct Mug implements Priced {
    let price: Int
}

extend<T> List<T> where T implements Priced {
    var totalPrice: Int {
        map { item => item.price }.count
    }
}

let mugs = [Mug(price: 3)]
let total = mugs.totalPrice
  //^^^^^ type: Int
let numbers = [1, 2]
let broken = numbers.totalPrice
