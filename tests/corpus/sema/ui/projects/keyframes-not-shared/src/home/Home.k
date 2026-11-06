// A parent can't reference a child's keyframes (06-styles.md): K3205.
import ui.Toast

public page Home {
    view {
        box {
            Toast()
        }
        .class(.wrap)
    }

    style {
        .wrap { animationName: .fade; }
    }
}
