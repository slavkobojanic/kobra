// Previews are a preview feature (`view-previews`): without
// `preview = ["view-previews"]` in k.toml each `preview` declaration is
// K8108 at its keyword, with the fix that adds the opt-in. The body is
// still checked, so the preview is ready once the project opts in.
public component Badge {
    view {
        "badge"
    }
}

preview {
    Badge()
}
