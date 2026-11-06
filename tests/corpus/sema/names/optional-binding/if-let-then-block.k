// `if let` binds the unwrapped value in the `then` block.
func length(of text: String?) -> Int {
    if let unwrapped = text {
    //                 ^^^^ resolves: param text 2:16
        return unwrapped.count
        //     ^^^^^^^^^ resolves: local unwrapped 3:12
    }
    return 0
}
