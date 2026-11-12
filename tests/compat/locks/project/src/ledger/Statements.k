/// One account's statements (`routes.lock`).
public page Statements at "/accounts/{name}/statements" {
    @Param let name: String

    view {
        heading(level: 1) { "Statements of {name}" }
    }
}
