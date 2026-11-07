// `count` counts user-perceived characters (grapheme clusters), not code units or scalars.
func main() -> List<Int> {
    return ["abc".count, "".count, "\u{E9}".count, "e\u{301}".count, "👍🏽".count, "🇳🇴".count, "a👍🏽b".count]
}
