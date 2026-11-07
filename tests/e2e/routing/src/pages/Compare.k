/// Two required parameters become two path segments, in declaration order.
public page Compare {
    @Param let left: Int
    @Param let right: Int

    view {
        paragraph { "compare {left} with {right}" }
    }
}
