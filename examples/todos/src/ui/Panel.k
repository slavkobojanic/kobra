/// A titled surface that groups related content, with optional slots for
/// header controls and footer actions.
///
/// `Panel` knows nothing about todos. It opts in to outside styling through a
/// single `@Stylable` hook, `container`, so a page can size and place the
/// panel without reaching into its internals.
///
/// ```k
/// Panel(title: "Groceries") {
///     paragraph { "Milk, eggs, bread." }
/// }
/// .actions {
///     button { "Clear" }
///         .on(click: clear)
/// }
/// ```
public component Panel {
    /// The heading shown at the top of the panel.
    @Prop let title: String

    /// The heading level of `title`. Use `1` when the panel is the page's
    /// main content.
    @Prop let headingLevel: Int = 2

    /// The panel's main content, passed as the trailing block.
    @Slot let content: View

    /// Controls shown at the end of the header, such as a settings switch.
    @Slot let toolbar: View? = nil

    /// Status and actions shown in the panel's footer.
    @Slot let actions: View? = nil

    /// Classes from the parent's `style` block, applied to the panel's outer
    /// element.
    @Stylable let container: Classes

    view {
        section {
            header {
                heading(level: headingLevel) { title }
                    .class(.title)

                if let toolbar {
                    toolbar
                }
            }
            .class(.header)

            content

            if let actions {
                footer { actions }
                    .class(.footer)
            }
        }
        .class([.panel, container])
    }

    style {
        .panel {
            display: grid;
            gap: .md;
            padding: .lg;
            border: 1px solid .border;
            borderRadius: .md;
            backgroundColor: .surface;

            @media (maxWidth: 600px) { padding: .md; }
        }

        .header {
            display: flex;
            flexWrap: wrap;
            alignItems: center;
            justifyContent: space-between;
            gap: .sm;
        }

        .title {
            margin: 0;
            fontSize: .xl;
        }

        .footer {
            display: flex;
            flexWrap: wrap;
            alignItems: center;
            gap: .md;
            paddingTop: .md;
            borderTop: 1px solid .border;
        }
    }
}
