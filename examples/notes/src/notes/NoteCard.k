import k.time.Clock
import k.ui.VisuallyHidden

/// One note in the list: its title, linking to the editor, the start of its
/// text, its tags, when it was last edited, and a pin toggle.
public component NoteCard {
    /// The note to show.
    @Prop let note: NoteSummary

    /// Fires with the note when the person presses the pin toggle. The
    /// parent decides what pinning means.
    @Emits let pinToggled: Event<NoteSummary>

    /// The reference instant for "Edited 5 minutes ago".
    @Inject let clock: Clock

    view {
        article {
            header {
                heading(level: 2) {
                    link(to: .editNote(id: note.id)) { note.title }
                }

                // Every card has a pin toggle, so screen readers also hear
                // which note it pins: "Pin Groceries".
                button {
                    "Pin"

                    VisuallyHidden { " {note.title}" }
                }
                .pressed(note.isPinned)
                .on(click: { pinToggled.emit(note) })
            }
            .class(.heading)

            if note.excerpt.isNotEmpty {
                paragraph { note.excerpt }
                    .class(.excerpt)
            }

            if note.tags.isNotEmpty {
                list {
                    for tag in note.tags key tag {
                        item { tag }
                            .class(.tag)
                    }
                }
                .class(.tags)
                .label("Tags")
            }

            text { "Edited {note.updatedAt.formatted(as: .relative, from: clock.now)}" }
                .class(.edited)
        }
        .class([.card, .pinned when note.isPinned])
    }

    style {
        .card {
            display: grid;
            gap: .sm;
            padding: .md;
            border: 1px solid .border;
            borderRadius: .md;
            backgroundColor: .surface;
        }

        .pinned  { borderColor: .highlight; }
        .heading { display: flex; alignItems: baseline; justifyContent: space-between; gap: .sm; }
        .excerpt { color: .muted; }
        .tags    { display: flex; flexWrap: wrap; gap: .xs; margin: 0; padding: 0; listStyle: none; }
        .tag     { padding: 0 .sm; borderRadius: .sm; backgroundColor: .surfaceRaised; fontSize: 0.875rem; }
        .edited  { color: .muted; fontSize: 0.875rem; }
    }
}
