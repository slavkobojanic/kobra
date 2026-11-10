import k.device.AppUpdates

/// Offers the newer build of the app once it has arrived.
///
/// The app is `@Offline`, so it opens from the build the browser keeps.
/// When a new build is deployed, the browser fetches it in the background
/// and `AppUpdates.isAvailable` turns `true`; reloading switches to it. The
/// todos are in local storage, so nothing is lost.
component UpdateNotice {
    @Inject let updates: AppUpdates

    view {
        if updates.isAvailable {
            box {
                paragraph { "A new version of Todos is ready." }
                    .class(.message)

                button { "Reload" }
                    .on(click: updates.apply)
            }
            .role(.status)
            .class(.notice)
        }
    }

    style {
        .notice {
            display: flex;
            alignItems: center;
            justifyContent: spaceBetween;
            gap: .md;
            padding: .sm .md;
            borderRadius: .sm;
            border: 1px solid .border;
            backgroundColor: .background;
            color: .text;
        }

        .message { margin: 0; }
    }
}
