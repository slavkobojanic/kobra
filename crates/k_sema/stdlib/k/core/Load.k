/// Marks an asynchronous [`Derived`] value loaded before `view` renders
/// (`docs/design/14-async-data.md`). Requires `let`, a `loading` block and a
/// [`Codable`] type; its errors go to the `failed` block.
@Annotation
public struct Load {}
