// A component may declare generic parameters.
component Picker<T> {
    @Prop let options: List<T>
    @Binding var selection: T?
}
