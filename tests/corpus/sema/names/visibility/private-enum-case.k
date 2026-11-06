// Enum cases are visible wherever their enum is: `private` on a case is K1301 at the modifier.
enum Light {
    case red
    private case green
}
