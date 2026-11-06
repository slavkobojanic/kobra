// Unicode text in line, block and doc comments is trivia.
/// Représente un utilisateur — 用户 👤
struct User { // ✓
    /* ñ /* 嵌套 */ ü */
    let id: Int
}
