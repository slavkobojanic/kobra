// The app's metadata sets no title, so a page without one of its own, or
// from its layout, is K3027 (22-metadata.md, "Rules").
import site.Plain

@Layout(Plain)
public app Site {
    metadata {
        locale(.english)
    }
}
