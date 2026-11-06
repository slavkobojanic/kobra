// `.style { ... }` after a `.` is a member with a trailing lambda, not a style block.
component T {
    view {
        box {}.style { compact }
    }
}
