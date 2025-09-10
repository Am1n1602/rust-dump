use tar::Builder;
use tempfile::NamedTempFile;
use std::fs::File;

/// Helper module for OCI push tests
pub mod oci_push_helpers {
    use super::*;

    /// Creates a minimal OCI tarball in a temporary file.
    /// Includes: config.json, manifest.json, and empty layer.tar
    pub fn create_fake_oci_tar() -> std::io::Result<NamedTempFile> {
        let temp = NamedTempFile::new()?;
        let file = File::create(temp.path())?;
        let mut builder = Builder::new(file);

        // --- config.json ---
        let config_json = r#"{
          "created": "2025-09-10T00:00:00Z",
          "architecture": "amd64",
          "os": "linux",
          "config": { "Env": ["PATH=/usr/local/bin"] },
          "rootfs": { "type": "layers", "diff_ids": ["sha256:dummyhash"] }
        }"#;
        let mut header = tar::Header::new_gnu();
        header.set_size(config_json.len() as u64);
        header.set_cksum();
        builder.append_data(&mut header, "config.json", config_json.as_bytes())?;

        // --- manifest.json ---
        let manifest_json = r#"{
          "schemaVersion": 2,
          "mediaType": "application/vnd.oci.image.manifest.v1+json",
          "config": {
            "mediaType": "application/vnd.oci.image.config.v1+json",
            "digest": "sha256:dummyhash",
            "size": 123
          },
          "layers": [
            {
              "mediaType": "application/vnd.oci.image.layer.v1.tar",
              "digest": "sha256:dummyhash",
              "size": 456
            }
          ]
        }"#;
        let mut header = tar::Header::new_gnu();
        header.set_size(manifest_json.len() as u64);
        header.set_cksum();
        builder.append_data(&mut header, "manifest.json", manifest_json.as_bytes())?;

        // --- empty layer.tar ---
        let empty_layer: Vec<u8> = Vec::new();
        let mut header = tar::Header::new_gnu();
        header.set_size(empty_layer.len() as u64);
        header.set_cksum();
        builder.append_data(&mut header, "layer.tar", &empty_layer[..])?;

        builder.finish()?;
        Ok(temp)
    }
}
