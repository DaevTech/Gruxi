use log::trace;
use tokio::sync::OnceCell;
use wildcard::{Wildcard, WildcardBuilder};

pub struct BlockedFilePatternMatching {
    wildcards: Vec<Wildcard<'static>>,
}

impl BlockedFilePatternMatching {
    pub async fn new() -> Self {
        let cached_configuration = crate::configuration::cached_configuration::get_cached_configuration();
        let config = cached_configuration.get_configuration().await;

        trace!("Initializing blocked file pattern matching with patterns: {:?}", config.core.server_settings.blocked_file_patterns);

        let patterns: Vec<String> = config.core.server_settings.blocked_file_patterns.clone();
        let wildcards = patterns
            .iter()
            .map(|p| {
                // Leak the string to get a 'static reference
                let static_str: &'static str = Box::leak(p.clone().into_boxed_str());
                WildcardBuilder::new(static_str.as_bytes()).case_insensitive(true).build().unwrap()
            })
            .collect();

        BlockedFilePatternMatching { wildcards }
    }

    pub fn is_file_pattern_blocked(&self, file_name: &str) -> bool {
        trace!("Checking if file pattern is blocked for file: {}", file_name);
        for wc in &self.wildcards {
            if wc.is_match(file_name.as_bytes()) {
                trace!("File pattern matched blocked pattern: {:?}", wc.pattern());
                return true;
            }
        }
        false
    }
}

static BLOCKED_FILE_PATTERN_MATCHING_SINGLETON: OnceCell<BlockedFilePatternMatching> = OnceCell::const_new();

pub async fn get_blocked_file_pattern_matching() -> &'static BlockedFilePatternMatching {
    BLOCKED_FILE_PATTERN_MATCHING_SINGLETON.get_or_init(|| async { BlockedFilePatternMatching::new().await }).await
}

pub struct WhitelistedFilePatternMatching {
    wildcards: Vec<Wildcard<'static>>,
}

impl WhitelistedFilePatternMatching {
    pub async fn new() -> Self {
        let cached_configuration = crate::configuration::cached_configuration::get_cached_configuration();
        let config = cached_configuration.get_configuration().await;

        trace!(
            "Initializing whitelisted file pattern matching with patterns: {:?}",
            config.core.server_settings.whitelisted_file_patterns
        );

        let patterns: Vec<String> = config.core.server_settings.whitelisted_file_patterns.clone();
        let wildcards = patterns
            .iter()
            .map(|p| {
                // Leak the string to get a 'static reference
                let static_str: &'static str = Box::leak(p.clone().into_boxed_str());
                WildcardBuilder::new(static_str.as_bytes()).case_insensitive(true).build().unwrap()
            })
            .collect();

        WhitelistedFilePatternMatching { wildcards }
    }

    pub fn is_file_pattern_whitelisted(&self, file_name: &str) -> bool {
        trace!("Checking if file pattern is whitelisted for file: {}", file_name);
        for wc in &self.wildcards {
            if wc.is_match(file_name.as_bytes()) {
                trace!("File pattern matched whitelisted pattern: {:?}", wc.pattern());
                return true;
            }
        }
        false
    }
}

static WHITELISTED_FILE_PATTERN_MATCHING_SINGLETON: OnceCell<WhitelistedFilePatternMatching> = OnceCell::const_new();

pub async fn get_whitelisted_file_pattern_matching() -> &'static WhitelistedFilePatternMatching {
    WHITELISTED_FILE_PATTERN_MATCHING_SINGLETON.get_or_init(|| async { WhitelistedFilePatternMatching::new().await }).await
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn test_blocked_file_pattern_matching() {
    let blocked_matching = get_blocked_file_pattern_matching().await;
    assert!(blocked_matching.is_file_pattern_blocked("index.php"));
    assert!(blocked_matching.is_file_pattern_blocked("test.tmp"));
    assert!(blocked_matching.is_file_pattern_blocked(".env"));
    assert!(blocked_matching.is_file_pattern_blocked(".env.example"));
    assert!(blocked_matching.is_file_pattern_blocked(".web.config"));
    assert!(blocked_matching.is_file_pattern_blocked("web.config"));
    assert!(!blocked_matching.is_file_pattern_blocked("index.html"));
    assert!(!blocked_matching.is_file_pattern_blocked("index.css"));
    assert!(blocked_matching.is_file_pattern_blocked("index.php.bak"));
    assert!(blocked_matching.is_file_pattern_blocked("mylog.log"));
    assert!(blocked_matching.is_file_pattern_blocked(".DS_Store"));
    assert!(blocked_matching.is_file_pattern_blocked(".whatever"));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn test_whitelisted_file_pattern_matching() {
    let whitelisted_matching = get_whitelisted_file_pattern_matching().await;
    assert!(whitelisted_matching.is_file_pattern_whitelisted("/var/www/html/.well-known/acme-challenge/token"));
    assert!(!whitelisted_matching.is_file_pattern_whitelisted("/var/www/html/.DS_STORE"));
    assert!(!whitelisted_matching.is_file_pattern_whitelisted("/var/www/html/.env"));
}
