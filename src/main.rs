
use anyhow::{Context, Result};
use clap::Parser;
use oci_distribution::{
    client::{Client, Config, ImageLayer},
    manifest,
    secrets::RegistryAuth,
    Reference,
};
use sha2::{Digest, Sha256};
use std::fs::File;
use std::io::Read;
use tar::Archive;

#[derive(Parser, Debug)]
struct Args {
    /// Path to OCI image tarball
    #[clap(short, long)]
    tarball: String,

    /// Registry reference (e.g., registry.example.com/repo:tag)
    #[clap(short, long)]
    reference: String,

    /// Registry username
    #[clap(long)]
    username: Option<String>,

    /// Registry password
    #[clap(long)]
    password: Option<String>,
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();

    // Unpack tarball
    let mut archive = Archive::new(
        File::open(&args.tarball)
            .with_context(|| format!("Failed to open OCI tarball: {}", args.tarball))?,
    );

    let mut layers: Vec<ImageLayer> = Vec::new();
    let mut config_bytes: Option<Vec<u8>> = None;

    for entry_res in archive.entries()? {
        let mut entry = entry_res?;
        let path = entry.path()?.to_string_lossy().to_string();
        let mut buf = Vec::new();
        entry.read_to_end(&mut buf)?;

        if path.ends_with(".tar") || path.ends_with(".tar.gz") {
            layers.push(ImageLayer {
                data: buf,
                media_type: "application/vnd.oci.image.layer.v1.tar".to_string(),
                annotations: None,
            });
        } else if path.ends_with("config.json") {
            config_bytes = Some(buf);
        }
    }

    let config_data = config_bytes.context("config.json not found in OCI tarball")?;

    // Digest helper
    let compute_digest = |data: &[u8]| -> String {
        let mut hasher = Sha256::new();
        hasher.update(data);
        format!("sha256:{:x}", hasher.finalize())
    };

    // Build manifest
    let img_manifest = manifest::OciImageManifest {
        schema_version: 2,
        media_type: Some("application/vnd.oci.image.manifest.v1+json".to_string()),
        artifact_type: None,
        config: manifest::OciDescriptor {
            media_type: "application/vnd.oci.image.config.v1+json".to_string(),
            digest: compute_digest(&config_data),
            size: config_data.len() as i64,
            annotations: None,
            urls: None,
        },
        layers: layers
            .iter()
            .map(|layer| manifest::OciDescriptor {
                media_type: layer.media_type.clone(),
                digest: compute_digest(&layer.data),
                size: layer.data.len() as i64,
                annotations: None,
                urls: None,
            })
            .collect(),
        annotations: None,
    };

    // Config wrapper for push()
    let config = Config {
        data: config_data.clone(),
        media_type: "application/vnd.oci.image.config.v1+json".to_string(),
        annotations: None,
    };

    // Registry client
    let client = Client::default();
    let reference: Reference = args
        .reference
        .parse()
        .with_context(|| format!("Invalid reference: {}", args.reference))?;

    let auth = match (&args.username, &args.password) {
        (Some(u), Some(p)) => RegistryAuth::Basic(u.clone(), p.clone()),
        _ => RegistryAuth::Anonymous,
    };

    // Push with retries
    let mut attempts = 0;
    const MAX_RETRIES: u8 = 3;

    loop {
        attempts += 1;
        match client
            .push(&reference, &layers, config.clone(), &auth, Some(img_manifest.clone()),)
            .await
        {
            Ok(_) => {
                println!("✅ Successfully pushed image to {}", args.reference);
                break;
            }
            Err(e) if attempts < MAX_RETRIES => {
                eprintln!("Push failed (attempt {}), retrying: {:?}", attempts, e);
            }
            Err(e) => {
                return Err(anyhow::anyhow!("Failed after {} attempts: {:?}", attempts, e));
            }
        }
    }

    Ok(())
}
