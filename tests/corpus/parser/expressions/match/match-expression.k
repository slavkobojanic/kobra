// the control-flow example of grammar.md section 19.3
let label = match status {
    case .active { "On" }
    case .shipped(let tracking) where tracking.isDelayed { "Late" }
    default { "Off" }
}
