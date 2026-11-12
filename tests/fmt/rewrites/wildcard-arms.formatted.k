func describe(n: Int, tone: Tone) -> String {
    let a = match tone {
        case .neutral { "neutral" }
        default { "other" }
    }
    let b = match n {
        case 0 { "zero" }
        case _ where n > 10 { "big" }
        default { "small" }
    }
    let c = match (n, tone) {
        case (_, .neutral) { "n" }
        default { "x" }
    }
    return a + b + c
}
