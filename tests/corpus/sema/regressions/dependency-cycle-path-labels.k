// K4003 labels the injection path of the cycle (diagnostics.md, services). In a
// cycle of three the message must not claim that two services depend on each
// other: `S1` doesn't inject `S3`.
service S1 {
    @Inject let s2: S2
}

service S2 {
    @Inject let s3: S3
}

service S3 {
    @Inject let s1: S1
}
