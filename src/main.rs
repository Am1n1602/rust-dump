use anyhow::{Context, Result};
use clap::Parser;
use oci_distribution::client::{Client, ClientConfig, ClientProtocol, Config, ImageLayer};
use oci_distribution::manifest::{OciDescriptor, OciImageManifest};
use oci_distribution::secrets::RegistryAuth;
use oci_distribution::Reference;
use sha2::{Digest, Sha256};
use std::fs::File;
use std::io::Read;
use tar::Archive;

#[derive(Parser, Debug)]
struct Args {
    /// Path to OCI tarball or Docker save tarball
    #[clap(short, long)]
    tarball: String,

    /// Registry reference (e.g., registry.example.com/repo:tag)
    #[clap(short, long)]
    reference: String,

    /// Registry username (optional)
    #[clap(long)]
    username: Option<String>,

    /// Registry password (optional)
    #[clap(long)]
    password: Option<String>,
}

/// Compute SHA-256 digest
fn compute_digest(data: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(data);
    format!("sha256:{:x}", hasher.finalize())
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();

    // Open tarball
    let file = File::open(&args.tarball)
        .with_context(|| format!("Failed to open tarball: {}", args.tarball))?;
    let mut archive = Archive::new(file);

    let mut layers: Vec<ImageLayer> = Vec::new();
    let mut config_bytes: Option<Vec<u8>> = None;

    // Extract config.json & layers
    for entry_res in archive.entries()? {
        let mut entry = entry_res?;
        let path = entry.path()?.to_string_lossy().to_string();
        let mut buf = Vec::new();
        entry.read_to_end(&mut buf)?;

        // Detect config.json (may be nested in Docker save tar)
        if path.ends_with("config.json") {
            config_bytes = Some(buf);
        }
        // Detect layer tar files
        else if path.ends_with(".tar") || path.ends_with(".tar.gz") {
            layers.push(ImageLayer {
                data: buf,
                media_type: "application/vnd.oci.image.layer.v1.tar".to_string(),
                annotations: None,
            });
        }
    }

    let config_data = config_bytes.unwrap_or_else(|| {
        println!("⚠️ config.json not found, generating minimal config...");
        b"{}".to_vec() // minimal config for testing
    });

    // Wrap config for push
    let config = Config {
        data: config_data.clone(),
        media_type: "application/vnd.oci.image.config.v1+json".to_string(),
        annotations: None,
    };

    // Compute digests
    let config_digest = compute_digest(&config_data);
    let layers_descriptors: Vec<OciDescriptor> = layers
        .iter()
        .map(|layer| OciDescriptor {
            media_type: layer.media_type.clone(),
            digest: compute_digest(&layer.data),
            size: layer.data.len() as i64,
            annotations: None,
            urls: None,
        })
        .collect();

    // Build manifest
    let manifest = OciImageManifest {
        schema_version: 2,
        media_type: Some("application/vnd.oci.image.manifest.v1+json".to_string()),
        artifact_type: None,
        config: OciDescriptor {
            media_type: "application/vnd.oci.image.config.v1+json".to_string(),
            digest: config_digest,
            size: config_data.len() as i64,
            annotations: None,
            urls: None,
        },
        layers: layers_descriptors,
        annotations: None,
    };

    // Parse registry reference
    let reference: Reference = args
        .reference
        .parse()
        .with_context(|| format!("Invalid reference: {}", args.reference))?;

    // Auth
    let auth = match (&args.username, &args.password) {
        (Some(u), Some(p)) => RegistryAuth::Basic(u.clone(), p.clone()),
        _ => RegistryAuth::Anonymous,
    };

    // Registry client
    let client_config = ClientConfig {
        protocol: ClientProtocol::Http, // Use Http for local registry
        accept_invalid_hostnames: true,
        accept_invalid_certificates: true,
        extra_root_certificates: vec![],
        platform_resolver: None,
        max_concurrent_upload: 4,
        max_concurrent_download: 4,
    };
    let client = Client::new(client_config);

    // Push with retries
    const MAX_RETRIES: u8 = 3;
    for attempt in 1..=MAX_RETRIES {
        match client
            .push(&reference, &layers, config.clone(), &auth, Some(manifest.clone()))
            .await
        {
            Ok(_) => {
                println!("✅ Successfully pushed image to {}", args.reference);
                break;
            }
            Err(e) if attempt < MAX_RETRIES => {
                eprintln!("⚠️ Push failed (attempt {}), retrying: {:?}", attempt, e);
            }
            Err(e) => {
                return Err(anyhow::anyhow!(
                    "❌ Failed after {} attempts: {:?}",
                    attempt,
                    e
                ));
            }
        }
    }

    Ok(())
}
