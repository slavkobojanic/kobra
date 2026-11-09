/// One tip percentage, as a button that shows whether it is the one in use.
public component TipOption {
    /// The tip this button offers, in percent.
    @Prop let percent: Int
    /// Whether this is the tip in use.
    @Prop let isSelected: Bool
    /// Fired with `percent` when the button is clicked.
    @Emits let chosen: Event<Int>

    view {
        button { "{percent}%" }
            .pressed(isSelected)
            .class([.option, .selected when isSelected])
            .on(click: { chosen.emit(percent) })
    }

    style {
        .option {
            padding: .sm .lg;
            border: 1px solid .primary;
            borderRadius: .sm;
            backgroundColor: transparent;
            color: .primary;
        }

        .selected {
            backgroundColor: .primary;
            color: white;
        }
    }
}
