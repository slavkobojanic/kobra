import k.device.ShareContent
import k.device.ShareError
import k.test.TestClipboard
import k.test.TestShare

test PasteService {
    @Inject let clipboard: TestClipboard
    @Inject let share: TestShare
    @Inject let service: PasteService

    test "a write puts the text on the clipboard" {
        let outcome = await service.copy("hello")

        expect outcome == "copied"
        expect clipboard.text == "hello"
    }

    test "a failed write is the error the double was given" {
        clipboard.failWrites(error: .denied)

        expect await service.copy("hello") == "denied"
    }

    test "a paste reads what another application copied" {
        clipboard.set(text: "from elsewhere")

        await service.paste()

        expect service.pasted == "from elsewhere"
    }

    test "an empty clipboard pastes no text" {
        await service.paste()

        expect service.pasted == "(no text)"
    }

    test "a refused read is denied" {
        clipboard.failReads(error: .denied)

        await service.paste()

        expect service.pasted == "(denied)"
    }

    test "without a clipboard, isSupported is false and calls are unsupported" {
        clipboard.setSupported(false)

        expect not service.canPaste
        expect await service.copy("hello") == "unsupported"
        await service.paste()
        expect service.pasted == "(unsupported)"
    }

    test "sharing a page records it with the page's link" {
        let outcome = await service.sharePage(title: "Home")

        expect outcome == "shared"
        expect share.shared.count == 1
        expect share.shared[0].title == "Home"
        expect share.shared[0].text == nil
        expect share.shared[0].url != nil
    }

    test "the next share can be cancelled or fail, once" {
        share.cancelNext()
        expect await service.sharePage(title: "Home") == "cancelled"

        share.failNext(error: .denied)
        expect await service.sharePage(title: "Home") == "denied"

        expect await service.sharePage(title: "Home") == "shared"
        expect share.shared.count == 1
    }

    test "without a share sheet every share is unsupported" {
        share.setSupported(false)

        expect not share.isSupported
        expect throws ShareError.unsupported {
            try await service.shareText("hi")
        }
    }

    test "text content carries the text alone" {
        try await service.shareText("hi")

        expect share.shared == [ShareContent.text("hi")]
    }
}
