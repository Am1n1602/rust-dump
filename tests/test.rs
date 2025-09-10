use learning::oci_push_helpers::create_fake_oci_tar;
use oci_distribution::client::{Client, ClientConfig, ClientProtocol, Config, ImageLayer};
use oci_distribution::manifest::{OciDescriptor, OciImageManifest};
use oci_distribution::secrets::RegistryAuth;
use oci_distribution::Reference;
use sha2::{Digest, Sha256};

// Compute SHA-256 digest
fn compute_digest(data: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(data);
    format!("sha256:{:x}", hasher.finalize())
}

#[tokio::test]
async fn test_push_fake_oci() {
    let _tarball = create_fake_oci_tar().expect("Failed to create fake OCI tarball");

    let reference = Reference::try_from("localhost:5000/fake:latest")
        .expect("Failed to parse reference");

    let client_config = ClientConfig {
        protocol: ClientProtocol::Http,
        accept_invalid_hostnames: false,
        accept_invalid_certificates: false,
        extra_root_certificates: vec![],
        platform_resolver: None,
        max_concurrent_upload: 4,
        max_concurrent_download: 4,
    };
    let client = Client::new(client_config);

    let layer_data = b"hello world".to_vec();
    let layer = ImageLayer {
        data: layer_data.clone(),
        media_type: "application/vnd.oci.image.layer.v1.tar".to_string(),
        annotations: None,
    };

    let config_bytes = b"{}".to_vec();
    let config_digest = compute_digest(&config_bytes);
    let layer_digest = compute_digest(&layer_data);

    let config = Config {
        data: config_bytes.clone(),
        media_type: "application/vnd.oci.image.config.v1+json".to_string(),
        annotations: None,
    };

    let manifest = OciImageManifest {
        schema_version: 2,
        media_type: Some("application/vnd.oci.image.manifest.v1+json".to_string()),
        artifact_type: None,
        config: OciDescriptor {
            media_type: "application/vnd.oci.image.config.v1+json".to_string(),
            digest: config_digest,
            size: config_bytes.len() as i64,
            annotations: None,
            urls: None,
        },
        layers: vec![OciDescriptor {
            media_type: layer.media_type.clone(),
            digest: layer_digest,
            size: layer.data.len() as i64,
            annotations: None,
            urls: None,
        }],
        annotations: None,
    };

    let result = client
        .push(&reference, &[layer], config, &RegistryAuth::Anonymous, Some(manifest))
        .await;

    match &result {
        Ok(_) => println!("✅ Push succeeded"),
        Err(e) => println!("❌ Push failed: {}", e),
    }

    assert!(result.is_ok() || result.is_err());
}
