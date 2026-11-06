// commas separate patterns of one arm
func f() {
    match state {
        case .idle, .loading { spinner() }
        case .failed(let e), .cancelled(let e) { show(e) }
    }
}
