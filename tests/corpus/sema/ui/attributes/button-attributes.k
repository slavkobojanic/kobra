// `button` defines `.type`, `.pressed`, `.expanded` and `.disabled`; implicit
// members in their values resolve against the attribute's type.
component Toolbar {
    @State var isSubmitting: Bool = false
    @State var isBold: Bool = false

    view {
        button { "Pay" }
            .type(.submit)
           //^^^^ resolves: attribute button.type
            //     ^^^^^^ resolves: member k.ui.ButtonType.submit
            .disabled(isSubmitting)
           //^^^^^^^^ resolves: attribute button.disabled
        button { "Bold" }
            .pressed(isBold)
            .expanded(false)
            .type(.button)
    }
}
