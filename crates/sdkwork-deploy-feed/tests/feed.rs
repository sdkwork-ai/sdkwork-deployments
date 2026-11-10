use sdkwork_deploy_contract::DownloadFeedDocument;
use sdkwork_deploy_feed::{
    build_feed, channel_filename, is_in_rollout, is_supported_channel, render_yaml, revision_of,
    stable_bucket, FeedError, FeedInput,
};

fn input() -> FeedInput {
    FeedInput {
        version: "0.2.2-rc.1".to_owned(),
        file_url: "https://download.example.com/bin/win-x64/BirdCoder-0.2.2-rc.1-win-x64.exe"
            .to_owned(),
        sha512: "YWJjZGVmZ2hpamtsbW5vcA==".to_owned(),
        size: 194_000_000,
        release_date: "2026-11-10T10:00:00.000Z".to_owned(),
        staging_percentage: Some(25),
    }
}

#[test]
fn channel_filenames_follow_the_updater_platform_rules() {
    assert_eq!(channel_filename("windows", "nightly"), "nightly.yml");
    assert_eq!(channel_filename("linux", "stable"), "stable.yml");
    assert_eq!(channel_filename("macos", "nightly"), "nightly-mac.yml");
    assert_eq!(channel_filename("macos", "stable"), "stable-mac.yml");
}

#[test]
fn only_protocol_channels_are_supported() {
    assert!(is_supported_channel("nightly"));
    assert!(is_supported_channel("stable"));
    assert!(!is_supported_channel("beta"));
    assert!(!is_supported_channel(""));
}

#[test]
fn rollout_bucket_is_stable_and_bounded() {
    let first = stable_bucket("install-1", "app.sdkwork.birdcoder", "0.2.2-rc.1");
    for _ in 0..10 {
        assert_eq!(
            stable_bucket("install-1", "app.sdkwork.birdcoder", "0.2.2-rc.1"),
            first
        );
    }
    assert!(first < 100);
    // A different installation is allowed to land in a different bucket, but
    // the function must stay total for arbitrary input.
    let other = stable_bucket("install-2", "app.sdkwork.birdcoder", "0.2.2-rc.1");
    assert!(other < 100);
}

#[test]
fn rollout_edges_are_total() {
    assert!(is_in_rollout("install", "product", "1.0.0", 100));
    assert!(!is_in_rollout("install", "product", "1.0.0", 0));
    assert!(!is_in_rollout("install", "product", "1.0.0", -5));
    assert!(is_in_rollout("install", "product", "1.0.0", 101));
}

#[test]
fn feed_requires_a_digest_and_a_url() {
    let mut missing_digest = input();
    missing_digest.sha512 = "  ".to_owned();
    assert!(matches!(
        build_feed(&missing_digest),
        Err(FeedError::MissingSha512)
    ));

    let mut missing_url = input();
    missing_url.file_url = String::new();
    assert!(matches!(
        build_feed(&missing_url),
        Err(FeedError::MissingFileUrl)
    ));
}

#[test]
fn yaml_is_the_exact_document_the_updater_reads() {
    let document = build_feed(&input()).expect("feed");
    let expected = concat!(
        "version: '0.2.2-rc.1'\n",
        "files:\n",
        "  - url: 'https://download.example.com/bin/win-x64/BirdCoder-0.2.2-rc.1-win-x64.exe'\n",
        "    sha512: 'YWJjZGVmZ2hpamtsbW5vcA=='\n",
        "    size: 194000000\n",
        "path: 'https://download.example.com/bin/win-x64/BirdCoder-0.2.2-rc.1-win-x64.exe'\n",
        "sha512: 'YWJjZGVmZ2hpamtsbW5vcA=='\n",
        "releaseDate: '2026-11-10T10:00:00.000Z'\n",
        "stagingPercentage: 25\n",
    );
    assert_eq!(render_yaml(&document), expected);
}

#[test]
fn yaml_omits_staging_when_fully_rolled_out() {
    let mut fully_rolled_out = input();
    fully_rolled_out.staging_percentage = None;
    let document = build_feed(&fully_rolled_out).expect("feed");
    assert!(!render_yaml(&document).contains("stagingPercentage"));
}

#[test]
fn revision_changes_with_the_published_version() {
    let first = build_feed(&input()).expect("feed");
    let mut next_input = input();
    next_input.version = "0.2.3-rc.1".to_owned();
    let second = build_feed(&next_input).expect("feed");
    assert_ne!(revision_of(&first), revision_of(&second));
    assert_eq!(
        revision_of(&DownloadFeedDocument::default()),
        revision_of(&DownloadFeedDocument::default())
    );
}
