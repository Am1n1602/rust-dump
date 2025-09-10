# -----------------------------
# CONFIG
# -----------------------------
$ImageName = "alpine:latest"
$OCI_Tar = "alpine-oci.tar"
$Registry = "localhost:5000"
$TargetReference = "$Registry/alpine:latest"

# Path to Rust project binary
$RustBinary = "target\debug\learning.exe"

# -----------------------------
# 1️⃣ Pull Docker image
# -----------------------------
Write-Host "`n[1] Pulling Docker image $ImageName..."
docker pull $ImageName

# -----------------------------
# 2️⃣ Create temporary container to export files
# -----------------------------
Write-Host "`n[2] Exporting Docker image filesystem..."
$ContainerId = docker create $ImageName
$TempDir = New-Item -ItemType Directory -Path (Join-Path $env:TEMP "oci-temp") -Force

docker export $ContainerId -o "$TempDir\rootfs.tar"
docker rm $ContainerId

# -----------------------------
# 3️⃣ Generate config.json & manifest.json for OCI
# -----------------------------
Write-Host "`n[3] Creating minimal OCI config and manifest..."
$configJson = @"
{
  "created": "$(Get-Date -Format "yyyy-MM-ddTHH:mm:ssZ")",
  "architecture": "amd64",
  "os": "linux",
  "config": { "Env": ["PATH=/usr/local/bin"] },
  "rootfs": { "type": "layers", "diff_ids": ["sha256:dummy hash"] }
}
"@

$manifestJson = @"
{
  "schemaVersion": 2,
  "mediaType": "application/vnd.oci.image.manifest.v1+json",
  "config": {
    "mediaType": "application/vnd.oci.image.config.v1+json",
    "digest": "sha256:dummy hash",
    "size": 123
  },
  "layers": [
    {
      "mediaType": "application/vnd.oci.image.layer.v1.tar",
      "digest": "sha256:dummy hash",
      "size": 456
    }
  ]
}
"@

Set-Content -Path "$TempDir\config.json" -Value $configJson
Set-Content -Path "$TempDir\manifest.json" -Value $manifestJson

# -----------------------------
# 4️⃣ Package OCI tarball
# -----------------------------
Write-Host "`n[4] Packaging OCI tarball..."
tar -cvf $OCI_Tar -C $TempDir .

# -----------------------------
# 5️⃣ Push using Rust app
# -----------------------------
Write-Host "`n[5] Running Rust push..."
& $RustBinary --tarball $OCI_Tar --reference $TargetReference

# -----------------------------
# 6️⃣ Verify pushed image in local registry
# -----------------------------
Write-Host "`n[6] Verifying pushed image..."
curl "http://$Registry/v2/alpine/tags/list" | ConvertFrom-Json
