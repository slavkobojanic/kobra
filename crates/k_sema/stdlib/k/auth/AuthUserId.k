/// Marks the app's user id type, the type of [`SessionUser.id`]. An app
/// declares exactly one type implementing it, which [`SessionUserId`]
/// stands for.
public interface AuthUserId implements Equatable, Hashable, Codable {}
