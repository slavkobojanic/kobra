// K2303 applies to every subject type, including ones that need a catch-all.
// The arm still counts as a catch-all, so there is no K2301 as well.
func size(count: Int) -> String {
    return match count {
        case 0 { "empty" }
        case _ { "some" }
    }
}
