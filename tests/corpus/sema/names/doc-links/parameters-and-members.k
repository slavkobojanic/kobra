// Links to the documented declaration's parameters and members resolve without a warning.

/// Adds [`amount`] to [`ledger`]'s total; see [`Ledger.total`].
func add(amount: Int, to ledger: Ledger) -> Int {
    return ledger.total + amount
}

/// A running total; [`total`] is the sum so far.
struct Ledger {
    let total: Int
}
