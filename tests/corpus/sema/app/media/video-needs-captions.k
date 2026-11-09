// A `video` playing a file or a URL needs captions (K3112, WCAG 1.2.2); a live stream is exempt,
// and `.captions(.notNeeded)` says a video has no speech (46-media-and-rtc.md, "Showing and
// playing media").
import k.device.File
import k.media.MediaController
import k.media.MediaStream

component Player {
    @Prop let clip: File
    @Prop let camera: MediaStream

    let player = MediaController()

    view {
        video(source: .file(clip))
        video(source: .url(Url("https://k.dev/talk.mp4")))
            .label("The talk")
        video(source: .stream(camera))
            .mirrored(true)
            .label("Your camera")
        video(source: .file(clip))
            .captions(.notNeeded)
            .controller(player)
        video(source: .url(Url("https://k.dev/talk.mp4")))
            .captions(.track(url: Url("https://k.dev/talk.vtt"), language: "en", label: "English"))
        audio(source: .file(clip))
            .label("Memo")
            .on(ended: { player.pause() })
            .on(playbackBlocked: { player.pause() })
    }
}
