// arm blocks hold several statements, and blank lines and comments separate arms
func f() {
    match event {
        // keyboard
        case .key(let k) {
            log(k)
            handle(k)
        }

        default {
            ignore()
        }
    }
}
