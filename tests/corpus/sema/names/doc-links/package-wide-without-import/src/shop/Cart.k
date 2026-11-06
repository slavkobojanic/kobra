// A link resolves against the whole package: another module's declaration needs no import, and test-only modules count.

/// Holds [`Product`] values; tests build carts from [`Samples`].
public struct Cart {
    let size: Int
}
