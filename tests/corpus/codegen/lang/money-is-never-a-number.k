// Money arithmetic goes through the runtime's exact Money type, never JS number arithmetic
// (docs/compiler/codegen.md §5).
func total(price: Money, quantity: Int) -> Money {
    return price * quantity + 0.50.usd
}
