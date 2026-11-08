import k.test.RecordingJobQueue

test MailQueue {
    @Inject let queue: RecordingJobQueue

    test "payloads are typed by the kind" {
        let all = queue.payloads(of: MailJob.kind)
        //  ^^^ type: List<mail.Mail>

        expect all.isEmpty
    }
}
