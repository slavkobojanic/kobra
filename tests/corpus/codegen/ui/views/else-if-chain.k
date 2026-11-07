// `else if` nests: each condition is evaluated only when the previous ones are false.
component Grade {
    @Prop let score: Int

    view {
        box {
            if score >= 90 {
                text { "A" }
            } else if score >= 50 {
                text { "B" }
            } else {
                text { "C" }
            }
        }
    }
}
