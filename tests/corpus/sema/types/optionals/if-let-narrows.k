// `if let` binds the unwrapped value in the then-branch.
func length(text: String?) -> Int {
    if let text {
        return text.count
             //^^^^ type: String
    }
    if let other = text {
         //^^^^^ type: String
        return other.count
    }
    return 0
}
