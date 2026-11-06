// `form.value` is readable in functions every reference to which comes from the
// action passed to `handle(…)` (sema.md §12.10). `loop` is called from `helper`
// (reached from the action) and from itself, so it qualifies; `orphan` is only
// called from itself and doesn't (K7001).
import k.validation.NotBlank
import k.validation.Validatable

struct SignInRequest implements Codable, Validatable {
    @NotBlank var email: String
}

page SignIn {
    @Form var credentials: SignInRequest = .empty

    func submit() async {
        helper()
    }

    func helper() {
        let r = credentials.value
        loop()
    }

    func loop() {
        let r = credentials.value
        loop()
    }

    func orphan() {
        let r = credentials.value
        orphan()
    }

    view {
        form {
            button { "Sign in" }.type(.submit)
        }
        .on(submit: credentials.handle(submit))
    }
}
