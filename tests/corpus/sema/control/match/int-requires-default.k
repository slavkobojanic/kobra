// `Int` can't be enumerated: literal arms are never exhaustive, so `default`
// is required (K2301 without it).
func size(count: Int) -> String {
    return match count {
        case 0 { "empty" }
        case 1 { "one" }
    }
}

func sizeWithDefault(count: Int) -> String {
    return match count {
        case 0 { "empty" }
        case 1 { "one" }
        default { "many" }
    }
}
