// `String` can't be enumerated either.
func greeting(language: String) -> String {
    return match language {
        case "en" { "Hello" }
        case "no" { "Hei" }
    }
}
