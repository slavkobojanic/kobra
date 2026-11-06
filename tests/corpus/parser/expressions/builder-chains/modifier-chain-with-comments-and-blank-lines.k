// comments and blank lines between chained lines do not end the chain
func f() {
    button("Save")
        // primary action

        .class(.primary) /* styled */
        .disabled(not form.isValid)
}
