/// Makes a struct follow the named-or-chained rule: fields without a
/// default are named arguments of its initializer, the others are chained
/// methods (`docs/design/01-principles.md`).
@Annotation
public struct Builder {}
