// A public struct exposing a constrained generic static function.
public struct Catalog {
    static func cheapest<T>(items: List<T>) -> T? where T implements Priced {
        return items.minBy { item => item.price }
    }
}
