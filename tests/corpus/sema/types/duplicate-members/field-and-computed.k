// A stored field and a computed property with the same name: K2019, three
// times for three declarations of `total`.
struct Order {
    let total: Int
    var total: Int {
        1
    }
    let total: Int
}
