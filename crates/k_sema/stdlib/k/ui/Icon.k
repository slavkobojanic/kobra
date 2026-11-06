/// An icon from the bundled icon set. Decorative unless given a `label`.
@Native
public component Icon {
    /// Which icon.
    @Prop let name: IconName
    /// The accessible name; `nil` hides the icon from assistive technology.
    @Prop let label: String? = nil
}
