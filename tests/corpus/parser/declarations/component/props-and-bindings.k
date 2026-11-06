// Required props, defaulted props, optional props and bindings.
component Stepper {
    @Binding var value: Int
    @Prop let step: Int = 1
    @Prop let max: Int? = nil
    @Prop let format: (Int) -> String = { n => "{n}" }
}
