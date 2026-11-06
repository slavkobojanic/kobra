/// The type with exactly one value, returned by functions that return
/// nothing.
///
/// `Event<Void>` is fired with `emit()`, and a job whose payload is `Void`
/// can run on a schedule. `Void` is `Codable` as `{}`.
@Native
public struct Void implements Equatable, Hashable, Codable {}
