// `@Rendering(.client)` turns server rendering off: the client renders instead of hydrating, and no
// page has a server renderer (23-app-structure.md, "Rendering mode").
@Rendering(.client)
public app Local {}
