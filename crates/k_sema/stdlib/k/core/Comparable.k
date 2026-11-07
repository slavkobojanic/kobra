/// A type whose values are ordered, so `<`, `<=`, `>` and `>=` apply and
/// collections can be sorted by it.
///
/// Numbers, [`String`], [`Date`], [`Duration`] and [`Money`] (within one
/// currency) are comparable. Enums and structs don't derive it.
public interface Comparable implements Equatable {}
