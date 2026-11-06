import k.validation.Range

/// The job worker's settings.
public config JobConfig {
    /// Jobs run concurrently per process.
    @Range(1...256) @Env("K_JOB_CONCURRENCY") let concurrency: Int = 4
}
