import k.device.TabChannel
import k.device.TabMessaging

let notices: TabChannel<String> = TabChannel(name: "notices")

/// Follows the notices other tabs post, while it is shown.
public service NoticeService {
    @Inject let tabs: TabMessaging

    @State var heard: List<String> = []

    func follow() async {
        for await notice in tabs.messages(on: notices) {
            heard.append(notice)
        }
    }
}
