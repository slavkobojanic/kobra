// Annotation arguments may be calls with their own labeled arguments.
struct Price {
    @Default(Money(amount: 0, currency: .usd)) var value: Money
}
