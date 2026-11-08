/// A dismissible notice: a live region with a message and a Dismiss
/// button, for storage and import problems.
public component Notice {
    /// What to tell the user.
    @Prop let message: String

    /// Fires when the user dismisses the notice.
    @Emits let dismissed: Event<Void>

    view {
        box {
            text { message }

            button { "Dismiss" }
                .type(.button)
                .on(click: { dismissed.emit() })
        }
        .role(.status)
        .class(.notice)
    }

    style {
        .notice {
            display: flex;
            alignItems: center;
            justifyContent: spaceBetween;
            gap: .md;
            padding: .sm .md;
            borderRadius: .sm;
            backgroundColor: .warningSurface;
            color: .warningText;
        }
    }
}
