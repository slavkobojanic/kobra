// Token values may be any expression: arithmetic, chains and calls.
theme T {
    space lg: Spacing = 16px * 2
    color overlay: Color = .primary.opacity(0.8)
    color mixed: Color = .primary.mix(.white, 20%)
    duration fast: Duration = 150ms
    shadow card: Shadow = Shadow(y: 1px, blur: 3px, color: #0000001A)
}
