// Generic parameters follow `extend`; `where` follows the type.
extend<T> List<T> where T implements Priced {
    func cheapest() -> T? {
        return self.minBy { item => item.price }
    }
}
