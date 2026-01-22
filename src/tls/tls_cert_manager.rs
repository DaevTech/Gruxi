use instant_acme::{Account, AuthorizationStatus, ChallengeType, Identifier, LetsEncrypt, NewAccount, NewOrder, OrderStatus};
use tokio::select;

use crate::{
    configuration::{binding::Binding, binding_site_relation::BindingSiteRelationship, load_configuration::fetch_configuration_in_db, site::Site},
    file::normalized_path::NormalizedPath,
    logging::syslog::{error, trace},
};

pub struct TlsCertManager {}

enum CertificateNeedRenewalReason {
    NotFound,
    InvalidCertificate,
    NearExpiry,
    Expired,
    NotValidYet,
    NoNeed,
}

impl TlsCertManager {
    pub async fn new() -> Self {
        TlsCertManager {}
    }

    pub async fn start_certificate_loop() {
        tokio::spawn(Self::certificate_handler_loop());
    }

    async fn get_acme_account() -> Result<Account, ()> {
        let cached_configuration = crate::configuration::cached_configuration::get_cached_configuration();
        let config = cached_configuration.get_configuration().await;

        let account_email = config.core.tls_settings.account_email.clone();
        let use_staging = config.core.tls_settings.use_staging_server;
        let mut alternative_cache_location = config.core.tls_settings.certificate_cache_path.clone();

        // If the alternative cache location is set, we run it through the normalized path
        let normalized_alternative_cache_location = NormalizedPath::new(&alternative_cache_location, "");
        if normalized_alternative_cache_location.is_err() {
            error(format!(
                "Invalid alternative certificate cache location specified: '{}'. Falling back to default 'certs/cache'",
                alternative_cache_location
            ));
            alternative_cache_location = String::new();
        } else {
            alternative_cache_location = normalized_alternative_cache_location.unwrap().get_full_path();
        }

        // First, we retrieve or create our ACME account
        if alternative_cache_location.is_empty() {
            let dir_created_result = std::fs::create_dir_all("certs/cache");
            if let Err(e) = dir_created_result {
                error(format!("Failed to create default certificate cache directory: {}", e));
                return Err(());
            }
        } else {
            let dir_created_result = std::fs::create_dir_all(&alternative_cache_location);
            if let Err(e) = dir_created_result {
                error(format!("Failed to create alternative certificate cache directory: {}", e));
                return Err(());
            }
        }

        // Add the filename to the location
        let account_location = if alternative_cache_location.is_empty() {
            "certs/cache/acme_account.json".to_string()
        } else {
            format!("{}/acme_account.json", alternative_cache_location)
        };

        // Start building the account
        let account_result = Account::builder();
        if account_result.is_err() {
            error(format!(
                "Failed to build ACME account builder - Consider removing existing account file to generate a new, located in {}",
                account_location
            ));
            return Err(());
        }
        let account = account_result.unwrap();

        // Check for existing account on disk
        if std::path::Path::new(&account_location).exists() {
            let account_data_result = std::fs::read_to_string(&account_location);
            if let Err(e) = account_data_result {
                error(format!(
                    "Failed to read existing ACME account file at '{}': {} - Consider removing existing account file to generate a new",
                    account_location, e
                ));
                return Err(());
            }
            let account_data = account_data_result.unwrap();

            let credentials_result = serde_json::from_str(&account_data);
            if let Err(e) = credentials_result {
                error(format!(
                    "Failed to parse existing ACME account credentials from file at '{}': {} - Consider removing existing account file to generate a new",
                    account_location, e
                ));
                return Err(());
            }
            let credentials = credentials_result.unwrap();

            let account_result = account.from_credentials(credentials).await;
            if account_result.is_err() {
                error("Failed to load ACME account from stored credentials");
                return Err(());
            }
            return Ok(account_result.unwrap());
        }

        // Account struct with contact mailto
        let mailto_contact = vec![format!("mailto:{}", &account_email)];
        let contacts: [&str; 1] = [mailto_contact[0].as_str()];

        let new_account = NewAccount {
            terms_of_service_agreed: true,
            contact: &contacts,
            only_return_existing: false,
        };

        // Determine which ACME directory to use
        let acme_dir = if use_staging { LetsEncrypt::Staging } else { LetsEncrypt::Production };

        let account_result = account.create(&new_account, acme_dir.url().to_string(), None).await;

        let account_tuple = match account_result {
            Ok(acc) => acc,
            Err(e) => {
                error(format!("Failed to create new ACME account: {}", e));
                return Err(());
            }
        };

        let (account, credentials) = account_tuple;

        // Save account credentials to disk
        let credentials_json_result = serde_json::to_string_pretty(&credentials);
        if let Err(e) = credentials_json_result {
            error(format!("Failed to serialize ACME account credentials to JSON: {}", e));
            return Err(());
        }
        let credentials_json = credentials_json_result.unwrap();
        let account_write_result = std::fs::write(&account_location, credentials_json);
        if let Err(e) = account_write_result {
            error(format!("Failed to write ACME account credentials to '{}': {}", account_location, e));
            return Err(());
        }
        Ok(account)
    }

    async fn certificate_handler_loop() {
        // Get shutdown and service stop tokens
        let triggers = crate::core::triggers::get_trigger_handler();
        let shutdown_token = triggers.get_trigger("shutdown").expect("Failed to get shutdown trigger").read().await.clone();
        let service_stop_token = triggers.get_trigger("stop_services").expect("Failed to get stop_services trigger").read().await.clone();

        // Setup our ACME account here if needed
        let acme_account_result = Self::get_acme_account().await;
        if acme_account_result.is_err() {
            error("TLS certificate manager failed to setup ACME account, automatic TLS will not function - Restart server or reload configuration to retry.".to_string());
        }
        let acme_account = acme_account_result.unwrap();
        trace("TLS certificate manager ACME account setup complete.".to_string());

        // For first run we wait 5 seconds, after that we can adjust the wait time
        let mut wait_until_next_run = std::time::Duration::from_secs(5);

        loop {
            select! {
                _ = tokio::time::sleep(wait_until_next_run) => {
                    trace("TLS certificate manager running certificate check...");

                    // We fetch a fresh configuration each time
                    let configuration_result = fetch_configuration_in_db();
                    if configuration_result.is_err() {
                        error(format!("TLS certificate manager failed to fetch valid configuration from database - Will retry: {}", configuration_result.err().unwrap()));
                        // We retry on next run
                        continue;
                    }
                    let configuration = configuration_result.unwrap();

                    let sites_to_update = Self::get_sites_needing_certificate_update(&configuration.sites, &configuration.bindings, &configuration.binding_sites).await;
                    let site_to_update_count = sites_to_update.len();
                    for site in sites_to_update {
                        trace(format!("Updating certificate for site: {:?}", site));
                        Self::update_site_certificate(&site, &acme_account).await;
                    }
                    if site_to_update_count == 0 {
                        trace("No TLS certificate updates needed at this time.".to_string());
                    }
                    wait_until_next_run = std::time::Duration::from_secs(10);
                },
                _ = shutdown_token.cancelled() => {
                    trace("TLS certificate manager received shutdown signal, exiting loop".to_string());
                    break;
                },
                _ = service_stop_token.cancelled() => {
                    trace("TLS certificate manager received stop services signal, exiting loop".to_string());
                    break;
                }
            }
        }
    }

    async fn update_site_certificate(site: &Site, acme_account: &Account) {
        // Prepare identifiers (domains) for the order
        let mut identifiers: Vec<Identifier> = vec![];
        for hostname in site.hostnames.iter() {
            identifiers.push(Identifier::Dns(hostname.clone()));
        }

        // Create the acme order
        let order_result = acme_account.new_order(&NewOrder::new(identifiers.as_slice())).await;
        if order_result.is_err() {
            error(format!("Failed to create new ACME order for site '{}' with domains: {:?}", site.id, site.hostnames));
            return;
        }
        let mut order = order_result.unwrap();
        let state = order.state();

        // Handle authorizations, which is the challenge/response process for each domain
        let mut authorizations = order.authorizations();

        // TODO: Implement challenge handling here
/*
        while let Some(result) = authorizations.next().await {
            let mut authz = result?;
            match authz.status {
                AuthorizationStatus::Pending => {}
                AuthorizationStatus::Valid => continue,
                _ => todo!(),
            }

            let mut challenge_option = authz.challenge(ChallengeType::Http01);


            challenge.set_ready().await?;
        }
       for authz_result in authorizations.collect::<Vec<_>>().await {
            let mut authz = match authz_result {
                Ok(a) => a,
                Err(e) => {
                    error(format!("Failed to retrieve authorization for ACME order for site '{}': {}", site.id, e));
                    return;
                }
            };
            match authz.status {
                AuthorizationStatus::Pending => {}
                AuthorizationStatus::Valid => continue,
                _ => {
                    error(format!("Authorization for ACME order for site '{}' is in unexpected status: {:?}", site.id, authz.status));
                    return;
                }
            }

            let mut challenge_option = authz.challenge(ChallengeType::Http01);
            if challenge_option.is_none() {
                error(format!("No HTTP-01 challenge found for authorization in ACME order for site '{}'", site.id));
                return;
            }
            let mut challenge = challenge_option.unwrap();

            // Here we would normally set up the HTTP-01 challenge response on our server
            // For this example, we assume it's done externally

            let challenge_set_result = challenge.set_ready().await;
            if let Err(e) = challenge_set_result {
                error(format!("Failed to set HTTP-01 challenge ready for site '{}': {}", site.id, e));
                return;
            }

            // Wait for authorization to be valid
            let authz_status_result = authz.wait_valid(Duration::from_secs(30)).await;
            if let Err(e) = authz_status_result {
                error(format!("Authorization did not become valid for site '{}': {}", site.id, e));
                return;
            }
        }
         */
        trace(format!("Updating TLS certificate for site ID: {}", site.id));
    }

    /// Get a list of sites that needs certificate update
    /// Site needs to be validated for automatic certificate management before being returned here
    async fn get_sites_needing_certificate_update(sites: &Vec<Site>, bindings: &Vec<Binding>, binding_sites: &Vec<BindingSiteRelationship>) -> Vec<Site> {
        let mut sites_to_update = Vec::new();

        // Besides being enabled, site must have automatic TLS enabled
        for site in sites.iter().filter(|s| !s.hostnames.is_empty() && s.is_enabled && s.tls_automatic_enabled) {
            // - Must have a valid domain name (not IP)
            let mut invalid_hostname_found = false;
            for hostname in site.hostnames.iter() {
                let verify_result = Site::verify_hostname(hostname);
                if verify_result.is_err() {
                    // Invalid hostname, skip this site
                    error(format!("Site '{}' has invalid hostname '{}', skipping automatic TLS", site.id, hostname));
                    invalid_hostname_found = true;
                    break;
                }
            }
            if invalid_hostname_found {
                continue;
            }

            // - Must have at least one binding and at least one on port 80 for the http-01 challenge
            let mut has_port_80_binding = false;
            for binding_site in binding_sites.iter().filter(|bs| bs.site_id == site.id) {
                let binding_opt = bindings.iter().find(|b| b.id == binding_site.binding_id);
                if let Some(binding) = binding_opt {
                    if binding.port == 80 {
                        has_port_80_binding = true;
                        break;
                    }
                }
            }
            if !has_port_80_binding {
                error(format!("Site '{}' does not have a binding on port 80 for the HTTP-01 challenge, skipping automatic TLS", site.id));
                continue;
            }

            // - If no cert/key path is set, a certificate was never issued, so we need to issue one
            if site.tls_cert_path.is_empty() || site.tls_key_path.is_empty() {
                trace(format!("Site '{}' is missing TLS certificate or key, marking for automatic TLS update", site.id));
                sites_to_update.push(site.clone());
                continue;
            }

            // If there is an existing certificate, we check that it is older than X days since last successful update
            let current_time = chrono::Utc::now().timestamp() as u64;
            let min_age_of_certificate = 5 * 24 * 60 * 60; // 5 days in seconds

            if !site.tls_cert_path.is_empty() && !site.tls_key_path.is_empty() {
                if site.tls_automatic_last_update_success > 0 && current_time - site.tls_automatic_last_update_success >= min_age_of_certificate {
                    let should_renew = Self::check_certificate_validity_should_be_renewed(site, current_time);
                    match should_renew {
                        CertificateNeedRenewalReason::NoNeed => {
                            // No action needed
                        }
                        _ => {
                            trace(format!("Site '{}' TLS certificate needs renewal, marking for automatic TLS update", site.id));
                            sites_to_update.push(site.clone());
                        }
                    }
                }
            }
        }

        sites_to_update
    }

    fn check_certificate_validity_should_be_renewed(site: &Site, current_time: u64) -> CertificateNeedRenewalReason {
        // We check the actual validity of the certificate
        // We aim to renew when certificate has less that 1/3 of validy period left

        let cert_data = match std::fs::read_to_string(&site.tls_cert_path) {
            Ok(data) => data,
            Err(e) => {
                error(format!("Failed to read TLS certificate file for site '{}': {}. Attempting to get a new certificate.", site.id, e));
                return CertificateNeedRenewalReason::NotFound;
            }
        };

        // Get the PEM block
        let cert_parsed_result = x509_parser::pem::parse_x509_pem(cert_data.as_bytes());
        if cert_parsed_result.is_err() {
            error(format!("Failed to parse TLS certificate for site '{}'. Attempting to get a new certificate.", site.id));
            return CertificateNeedRenewalReason::InvalidCertificate;
        }
        let (_, pem) = cert_parsed_result.unwrap();

        // Parse the X.509 certificate
        let x509_result = pem.parse_x509();
        if x509_result.is_err() {
            error(format!("Failed to parse X.509 certificate for site '{}'. Attempting to get a new certificate.", site.id));
            return CertificateNeedRenewalReason::InvalidCertificate;
        }
        let x509 = x509_result.unwrap();
        let not_before = x509.tbs_certificate.validity.not_before.to_datetime().unix_timestamp() as u64;
        let not_after = x509.tbs_certificate.validity.not_after.to_datetime().unix_timestamp() as u64;

        // We just make sure it is not expired
        if current_time >= not_after {
            trace(format!("Site '{}' TLS certificate has expired, marking for automatic TLS update", site.id));
            return CertificateNeedRenewalReason::Expired;
        }
        // Or before the valid from date
        if current_time < not_before {
            trace(format!("Site '{}' TLS certificate is not yet valid, marking for automatic TLS update", site.id));
            return CertificateNeedRenewalReason::NotValidYet;
        }

        // Check if we are within 1/3 last part of the validity period
        let total_validity_period = not_after - not_before;
        let time_left = if current_time >= not_after { 0 } else { not_after - current_time };
        if time_left <= total_validity_period / 3 {
            trace(format!(
                "Site '{}' last successful TLS update was over 5 days ago and less than 1/3 of validity period left, marking for automatic TLS update",
                site.id
            ));
            return CertificateNeedRenewalReason::NearExpiry;
        }
        CertificateNeedRenewalReason::NoNeed
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test(flavor = "multi_thread", worker_threads = 1)]
    async fn test_get_sites_needing_certificate_update_invalid_hostnames() {
        let mut site = Site::new();
        site.tls_automatic_enabled = true;
        site.hostnames = vec!["validdomain.com".to_string(), "256.256.256.256".to_string()]; // Invalid IP
        let sites = vec![site];
        let bindings = vec![];
        let binding_sites = vec![];
        let result = TlsCertManager::get_sites_needing_certificate_update(&sites, &bindings, &binding_sites).await;
        assert!(result.is_empty(), "Expected no sites to be returned due to invalid hostname");
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 1)]
    async fn test_get_sites_needing_certificate_update_no_port_80_binding() {
        let mut site = Site::new();
        site.tls_automatic_enabled = true;
        site.hostnames = vec!["validdomain.com".to_string()];

        let mut binding = Binding::new();
        binding.port = 443; // No port 80

        let binding_site = BindingSiteRelationship {
            binding_id: binding.id.clone(),
            site_id: site.id.clone(),
        };
        let sites = vec![site];
        let bindings = vec![binding];
        let binding_sites = vec![binding_site];
        let result = TlsCertManager::get_sites_needing_certificate_update(&sites, &bindings, &binding_sites).await;
        assert!(result.is_empty(), "Expected no sites to be returned due to binding missing port 80");
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 1)]
    async fn test_get_sites_needing_certificate_update_empty_tls_cert_paths() {
        let mut site = Site::new();
        site.tls_automatic_enabled = true;
        site.hostnames = vec!["validdomain.com".to_string()];

        let binding = Binding::new();

        let binding_site = BindingSiteRelationship {
            binding_id: binding.id.clone(),
            site_id: site.id.clone(),
        };
        let sites = vec![site];
        let bindings = vec![binding];
        let binding_sites = vec![binding_site];
        let result = TlsCertManager::get_sites_needing_certificate_update(&sites, &bindings, &binding_sites).await;
        assert_eq!(result.len(), 1, "Expected one site to be returned due to empty TLS cert/key paths");
        assert_eq!(result[0].id, sites[0].id, "Returned site ID does not match expected");
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 1)]
    async fn test_get_sites_needing_certificate_update_valid_certificate() {
        let mut site = Site::new();
        site.tls_automatic_enabled = true;
        site.hostnames = vec!["validdomain.com".to_string()];
        site.tls_cert_path = "tests/testdata/example_certificate.pem".to_string();
        site.tls_key_path = "tests/testdata/example_key.pem".to_string();
        site.tls_automatic_last_update_success = 1;

        let binding = Binding::new();

        let binding_site = BindingSiteRelationship {
            binding_id: binding.id.clone(),
            site_id: site.id.clone(),
        };
        let sites = vec![site];
        let bindings = vec![binding];
        let binding_sites = vec![binding_site];
        let result = TlsCertManager::get_sites_needing_certificate_update(&sites, &bindings, &binding_sites).await;
        assert_eq!(result.len(), 0);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 1)]
    async fn test_get_sites_needing_certificate_update_certificate_not_found() {
        let mut site = Site::new();
        site.tls_automatic_enabled = true;
        site.hostnames = vec!["validdomain.com".to_string()];
        site.tls_cert_path = "tests/testdata/example_certificate_not_found.pem".to_string();
        site.tls_key_path = "tests/testdata/example_key.pem".to_string();
        site.tls_automatic_last_update_success = 1;

        let binding = Binding::new();

        let binding_site = BindingSiteRelationship {
            binding_id: binding.id.clone(),
            site_id: site.id.clone(),
        };
        let sites = vec![site];
        let bindings = vec![binding];
        let binding_sites = vec![binding_site];
        let result = TlsCertManager::get_sites_needing_certificate_update(&sites, &bindings, &binding_sites).await;
        assert_eq!(result.len(), 1);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 1)]
    async fn test_check_certificate_validity_should_be_renewed_not_found_disk() {
        let mut site = Site::new();
        site.tls_automatic_enabled = true;
        site.hostnames = vec!["validdomain.com".to_string()];
        site.tls_cert_path = "tests/testdata/example_certificate_not_found.pem".to_string();
        site.tls_key_path = "tests/testdata/example_key.pem".to_string();
        site.tls_automatic_last_update_success = 1;

        let result = TlsCertManager::check_certificate_validity_should_be_renewed(&site, chrono::Utc::now().timestamp() as u64);
        assert!(matches!(result, CertificateNeedRenewalReason::NotFound));
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 1)]
    async fn test_check_certificate_validity_valid() {
        let mut site = Site::new();
        site.tls_automatic_enabled = true;
        site.hostnames = vec!["validdomain.com".to_string()];
        site.tls_cert_path = "tests/testdata/example_certificate.pem".to_string();
        site.tls_key_path = "tests/testdata/example_key.pem".to_string();
        site.tls_automatic_last_update_success = 1;

        let result = TlsCertManager::check_certificate_validity_should_be_renewed(&site, chrono::Utc::now().timestamp() as u64);
        assert!(matches!(result, CertificateNeedRenewalReason::NoNeed));
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 1)]
    async fn test_check_certificate_validity_expired() {
        let mut site = Site::new();
        site.tls_automatic_enabled = true;
        site.hostnames = vec!["validdomain.com".to_string()];
        site.tls_cert_path = "tests/testdata/example_certificate.pem".to_string();
        site.tls_key_path = "tests/testdata/example_key.pem".to_string();
        site.tls_automatic_last_update_success = 1;

        let time_in_the_future_after_cert_expire = 2386227023;

        let result = TlsCertManager::check_certificate_validity_should_be_renewed(&site, time_in_the_future_after_cert_expire);
        assert!(matches!(result, CertificateNeedRenewalReason::Expired));
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 1)]
    async fn test_check_certificate_validity_before_valid() {
        let mut site = Site::new();
        site.tls_automatic_enabled = true;
        site.hostnames = vec!["validdomain.com".to_string()];
        site.tls_cert_path = "tests/testdata/example_certificate.pem".to_string();
        site.tls_key_path = "tests/testdata/example_key.pem".to_string();
        site.tls_automatic_last_update_success = 1;

        let time_in_the_future_after_cert_expire = 1522054223;

        let result = TlsCertManager::check_certificate_validity_should_be_renewed(&site, time_in_the_future_after_cert_expire);
        assert!(matches!(result, CertificateNeedRenewalReason::NotValidYet));
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 1)]
    async fn test_check_certificate_validity_within_start_period_validity() {
        let mut site = Site::new();
        site.tls_automatic_enabled = true;
        site.hostnames = vec!["validdomain.com".to_string()];
        site.tls_cert_path = "tests/testdata/example_certificate.pem".to_string();
        site.tls_key_path = "tests/testdata/example_key.pem".to_string();
        site.tls_automatic_last_update_success = 1;

        let time_in_the_future_after_cert_expire = 1616752223;

        let result = TlsCertManager::check_certificate_validity_should_be_renewed(&site, time_in_the_future_after_cert_expire);
        assert!(matches!(result, CertificateNeedRenewalReason::NoNeed));
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 1)]
    async fn test_check_certificate_validity_within_last_third_validity() {
        let mut site = Site::new();
        site.tls_automatic_enabled = true;
        site.hostnames = vec!["validdomain.com".to_string()];
        site.tls_cert_path = "tests/testdata/example_certificate.pem".to_string();
        site.tls_key_path = "tests/testdata/example_key.pem".to_string();
        site.tls_automatic_last_update_success = 1;

        let time_in_the_future_after_cert_expire = 2279440223;

        let result = TlsCertManager::check_certificate_validity_should_be_renewed(&site, time_in_the_future_after_cert_expire);
        assert!(matches!(result, CertificateNeedRenewalReason::NearExpiry));
    }
}
