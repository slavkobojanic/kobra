// `bind` nested inside an argument expression (not directly the argument value) is K0110.
component T {
    view {
        Stepper(value: (bind quantity))
    }
}
