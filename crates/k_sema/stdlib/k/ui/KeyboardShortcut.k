/// A single-key keyboard shortcut for the page while the component is
/// mounted (`docs/design/43-keyboard-shortcuts.md`): pressing `key` fires
/// `pressed`, unless the user is typing in a field or holds Control, Alt
/// or Meta. It renders nothing, so the control that does the same thing
/// should show the key, and the page should list its shortcuts.
@Native
public component KeyboardShortcut {
    /// The key, as the browser names it: a character ("n", "?") or a key
    /// name ("Escape"). Letters match in either case.
    @Prop let key: String
    /// What the shortcut does ("New card"), for the page's list of
    /// shortcuts.
    @Prop let label: String
    /// Fires when the key is pressed. It is a user's action, so its
    /// handler may open a file picker or a prompt
    /// ([`k.core.UserAction`]).
    @UserAction
    @Emits let pressed: Event<Void>
}
