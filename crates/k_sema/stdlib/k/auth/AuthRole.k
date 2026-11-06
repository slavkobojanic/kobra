/// Marks the app's role enum; `SessionUser.roles` is a `Set` of it. An app
/// declares at most one enum implementing it.
public interface AuthRole implements Equatable, Hashable, Codable {}
