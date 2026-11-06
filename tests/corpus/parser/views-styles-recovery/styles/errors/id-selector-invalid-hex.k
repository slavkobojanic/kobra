// `#header` as a selector is K0132, and also K0005 because it is not a valid color literal; it is wrapped in an ErrorNode.
component T {
    style {
        #header { color: red; }
    }
}
