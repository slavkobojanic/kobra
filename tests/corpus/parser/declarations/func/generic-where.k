// Generic constraints are written only in `where` clauses, after the return type.
func cheapest<T>(items: List<T>) -> T? where T implements Priced {
    return items.minBy { item => item.price }
}
