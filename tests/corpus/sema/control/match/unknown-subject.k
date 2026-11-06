// An unresolved subject is K1201 only: the arms aren't checked against an
// unknown type, so there is no K2301 or K2006.
func label() -> String {
    return match missing {
        case .red { "stop" }
    }
}
