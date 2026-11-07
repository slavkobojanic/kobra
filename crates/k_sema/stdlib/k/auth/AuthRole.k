/// Marks the app's role enum; [`SessionUser.roles`] is a `Set` of it. An
/// app declares at most one enum implementing it, which [`SessionRole`]
/// stands for.
public interface AuthRole implements Equatable, Hashable, Codable {}
