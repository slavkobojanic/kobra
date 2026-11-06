// `Secret` deliberately has no `==`, so a struct holding one can't derive `Equatable`.
struct Credentials implements Equatable {
    let user: String
    let password: Secret<String>
}
