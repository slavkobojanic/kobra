/// Marks an asynchronous [`Derived`] value loaded before `view` renders
/// (`docs/design/14-async-data.md`). Requires `let`, a `loading` block and a
/// [`Codable`] type; its errors go to the `failed` block.
@Annotation
public struct Load {
    /// `true` keeps the value on `reload()` while the load's inputs are
    /// unchanged: for values that don't change on their own, such as a place
    /// looked up by its id. An input change still runs it again.
    let keepOnReload: Bool = false
}
