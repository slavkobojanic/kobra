/// How loud a badge is.
enum Tone {
    case quiet
    case loud
}

/// A short label, emphasised on request.
public component Badge {
    @Prop let text: String
    @Prop let strong: Bool = false
    @Prop let tone: Tone = .quiet

    view {
        if strong {
            text { "Strong: {text}" }
        } else {
            text { "Plain: {text}" }
        }

        if tone == .loud {
            text { "Loud" }
        } else {
            text { "Quiet" }
        }
    }
}

// `k preview`'s tests toggle `strong` and pick `tone` (k_cli's preview
// tests); `k check` mounts it with the defaults.
preview (text: String = "New", strong: Bool = false, tone: Tone = .quiet) {
    Badge(text: text, strong: strong, tone: tone)
}
