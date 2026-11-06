// Validation annotations take several labeled arguments.
struct SignUp {
    @Email var email: String
    @Length(min: 8, max: 64) var password: String
    @NotBlank @Matches("^[a-z]+$") var handle: String
}
