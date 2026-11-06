/// Content that screen readers announce but that isn't shown, for adding
/// words to an accessible name (`docs/design/05-views.md`):
/// `button { "Delete"  VisuallyHidden { " {todo.title}" } }`.
@Native
public component VisuallyHidden {
    /// The hidden content.
    @Slot let content: View
}
