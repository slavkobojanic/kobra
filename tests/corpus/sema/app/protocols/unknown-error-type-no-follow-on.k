// An unresolved error type in `throws` reports K1202 only, not also K5502.
protocol Store {
    rpc add() throws CartEror -> Int
}
