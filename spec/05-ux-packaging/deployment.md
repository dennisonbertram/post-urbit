# Deployment

## Overview

This document covers deploying the node daemon across different platforms and environments. The goal is enabling self-hosting for a range of technical abilities.

## Deployment Tiers

| Tier | User Profile | Platform | Complexity |
|------|-------------|----------|------------|
| **1. Managed** | Non-technical | Hosted service | Click-to-deploy |
| **2. Docker** | Comfortable with CLI | Any with Docker | Single command |
| **3. Binary** | System admin | Linux/macOS/Windows | Manual config |
| **4. Source** | Developer | Any | Build from source |

## Tier 1: Managed Hosting

For users who want a personal node without self-hosting.

### Provider Requirements

Managed hosting providers must:
- Run official node binaries (or verified builds)
- Provide isolated instances per user
- Support data export (identity + data)
- Allow migration to self-hosted
- Publish security/privacy policy

### User Experience

```
1. Sign up at provider website
2. Create identity (or import existing)
3. Node is provisioned automatically
4. Access via web UI or mobile app
5. Data export available anytime
```

### Provider API (for migration)

```typescript
interface ManagedProvider {
  // Export all user data
  exportData(): Promise<{
    identity: EncryptedIdentityExport;
    messages: EncryptedMessagesExport;
    apps: AppDataExport[];
    config: NodeConfig;
  }>;

  // Transfer to another provider or self-hosted
  initiateTransfer(target: {
    type: 'provider' | 'self_hosted';
    endpoint?: string;
  }): Promise<TransferToken>;
}
```

## Tier 2: Docker Deployment

### Quick Start

```bash
# Pull and run (uses default config)
docker run -d \
  --name postnode \
  -p 4433:4433/udp \
  -p 8080:8080 \
  -v postnode_data:/data \
  ghcr.io/postnode/postnode:latest

# View logs
docker logs -f postnode

# Access admin UI
open http://localhost:8080
```

### Docker Compose

```yaml
# docker-compose.yml
version: '3.8'

services:
  postnode:
    image: ghcr.io/postnode/postnode:latest
    container_name: postnode
    restart: unless-stopped
    ports:
      - "4433:4433/udp"   # QUIC transport
      - "8080:8080"       # Admin UI (local only)
      # Uncomment for external access:
      # - "8443:8443"     # Admin UI (TLS)
    volumes:
      - postnode_data:/data
      - ./config.toml:/etc/postnode/config.toml:ro
    environment:
      - POSTNODE_LOG_LEVEL=info
      # Set admin token (generate with: openssl rand -hex 32)
      - POSTNODE_ADMIN_TOKEN_HASH=<sha256-hash>
    healthcheck:
      test: ["CMD", "wget", "-q", "--spider", "http://localhost:8080/health/live"]
      interval: 30s
      timeout: 10s
      retries: 3

volumes:
  postnode_data:
```

### Docker Image Variants

| Tag | Description | Size |
|-----|-------------|------|
| `latest` | Latest stable release | ~50 MB |
| `X.Y.Z` | Specific version | ~50 MB |
| `X.Y.Z-alpine` | Alpine-based (smaller) | ~30 MB |
| `X.Y.Z-debug` | With debugging tools | ~100 MB |
| `nightly` | Latest development build | ~50 MB |

### Resource Limits

```yaml
# Recommended resource limits
services:
  postnode:
    deploy:
      resources:
        limits:
          cpus: '2'
          memory: 1G
        reservations:
          cpus: '0.5'
          memory: 256M
```

## Tier 3: Binary Installation

### Linux (systemd)

```bash
# Download binary
curl -LO https://github.com/postnode/postnode/releases/latest/download/postnode-linux-amd64
chmod +x postnode-linux-amd64
sudo mv postnode-linux-amd64 /usr/local/bin/postnode

# Create user and directories
sudo useradd -r -s /bin/false postnode
sudo mkdir -p /var/lib/postnode /etc/postnode
sudo chown postnode:postnode /var/lib/postnode

# Create config
sudo cat > /etc/postnode/config.toml << 'EOF'
[node]
data_dir = "/var/lib/postnode"
log_level = "info"

[network]
listen_addr = "0.0.0.0:4433"
admin_listen_addr = "127.0.0.1:8080"

[admin]
enabled = true
EOF

# Create systemd service
sudo cat > /etc/systemd/system/postnode.service << 'EOF'
[Unit]
Description=Post Node Daemon
After=network.target

[Service]
Type=simple
User=postnode
Group=postnode
ExecStart=/usr/local/bin/postnode start --config /etc/postnode/config.toml
Restart=on-failure
RestartSec=5

# Security hardening
NoNewPrivileges=true
ProtectSystem=strict
ProtectHome=true
ReadWritePaths=/var/lib/postnode
PrivateTmp=true

[Install]
WantedBy=multi-user.target
EOF

# Enable and start
sudo systemctl daemon-reload
sudo systemctl enable postnode
sudo systemctl start postnode
```

### macOS (launchd)

```bash
# Install via Homebrew
brew install postnode/tap/postnode

# Or download binary manually
curl -LO https://github.com/postnode/postnode/releases/latest/download/postnode-darwin-arm64
chmod +x postnode-darwin-arm64
sudo mv postnode-darwin-arm64 /usr/local/bin/postnode

# Create launchd plist
cat > ~/Library/LaunchAgents/com.postnode.plist << 'EOF'
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>Label</key>
    <string>com.postnode</string>
    <key>ProgramArguments</key>
    <array>
        <string>/usr/local/bin/postnode</string>
        <string>start</string>
    </array>
    <key>RunAtLoad</key>
    <true/>
    <key>KeepAlive</key>
    <true/>
    <key>StandardOutPath</key>
    <string>/usr/local/var/log/postnode.log</string>
    <key>StandardErrorPath</key>
    <string>/usr/local/var/log/postnode.log</string>
</dict>
</plist>
EOF

# Load service
launchctl load ~/Library/LaunchAgents/com.postnode.plist
```

### Windows

```powershell
# Download binary
Invoke-WebRequest -Uri "https://github.com/postnode/postnode/releases/latest/download/postnode-windows-amd64.exe" -OutFile "postnode.exe"

# Move to Program Files
New-Item -ItemType Directory -Force -Path "C:\Program Files\PostNode"
Move-Item postnode.exe "C:\Program Files\PostNode\postnode.exe"

# Add to PATH
$env:Path += ";C:\Program Files\PostNode"
[Environment]::SetEnvironmentVariable("Path", $env:Path, [EnvironmentVariableTarget]::Machine)

# Install as Windows Service (requires NSSM)
nssm install PostNode "C:\Program Files\PostNode\postnode.exe" start
nssm set PostNode AppDirectory "C:\ProgramData\PostNode"
nssm set PostNode AppStdout "C:\ProgramData\PostNode\logs\postnode.log"
nssm set PostNode AppStderr "C:\ProgramData\PostNode\logs\postnode.log"

# Start service
nssm start PostNode
```

## Tier 4: Building from Source

### Prerequisites

| Dependency | Version | Purpose |
|------------|---------|---------|
| Rust | 1.75+ | Compiler |
| Git | 2.x | Source control |
| OpenSSL | 1.1+ | TLS (Linux) |

### Build Steps

```bash
# Clone repository
git clone https://github.com/postnode/postnode.git
cd postnode

# Build release binary
cargo build --release

# Binary is at target/release/postnode
./target/release/postnode --version

# Run tests
cargo test

# Build with specific features
cargo build --release --features "metrics,debug-logs"
```

### Cross-Compilation

```bash
# Install cross-compilation tools
cargo install cross

# Build for different targets
cross build --release --target aarch64-unknown-linux-gnu  # ARM64 Linux
cross build --release --target x86_64-apple-darwin        # Intel macOS
cross build --release --target aarch64-apple-darwin       # ARM macOS
```

## Network Configuration

### Firewall Rules

| Port | Protocol | Direction | Purpose |
|------|----------|-----------|---------|
| 4433 | UDP | Inbound | QUIC transport |
| 8080 | TCP | Local only | Admin UI (default) |
| 8443 | TCP | Inbound | Admin UI (TLS, if enabled) |

### NAT Traversal

For home networks behind NAT:

```toml
[network]
# Enable UPnP for automatic port forwarding
enable_upnp = true

# Or manual port forwarding:
# 1. Configure router to forward UDP 4433 to this machine
# 2. Set external address if known
external_addr = "1.2.3.4:4433"

# Use relay servers as fallback
relay_servers = [
  "relay1.postnode.org:4433",
  "relay2.postnode.org:4433"
]
```

### Reverse Proxy (for Admin UI)

Using nginx to expose Admin UI:

```nginx
# /etc/nginx/sites-available/postnode
server {
    listen 443 ssl http2;
    server_name node.example.com;

    ssl_certificate /etc/letsencrypt/live/node.example.com/fullchain.pem;
    ssl_certificate_key /etc/letsencrypt/live/node.example.com/privkey.pem;

    location / {
        proxy_pass http://127.0.0.1:8080;
        proxy_set_header Host $host;
        proxy_set_header X-Real-IP $remote_addr;
        proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for;
        proxy_set_header X-Forwarded-Proto $scheme;

        # WebSocket support
        proxy_http_version 1.1;
        proxy_set_header Upgrade $http_upgrade;
        proxy_set_header Connection "upgrade";
    }
}
```

## Hardware Recommendations

### Minimum Requirements

| Component | Minimum | Recommended |
|-----------|---------|-------------|
| CPU | 1 core, 1 GHz | 2 cores, 2 GHz |
| RAM | 512 MB | 2 GB |
| Storage | 1 GB | 10 GB+ |
| Network | 1 Mbps | 10 Mbps |

### Tested Platforms

| Platform | Status | Notes |
|----------|--------|-------|
| Raspberry Pi 4 (4GB) | Supported | Recommended for home |
| Raspberry Pi 5 | Supported | Best Pi experience |
| Intel NUC | Supported | Good performance |
| AWS t3.micro | Supported | Free tier eligible |
| DigitalOcean Basic | Supported | $4/month option |
| Synology NAS | Community | Via Docker |
| QNAP NAS | Community | Via Docker |

## First-Run Setup

### Interactive Setup

```
$ postnode start

Welcome to Post Node!

No identity found. Let's set one up.

? Choose setup method:
  > Create new identity
    Import existing identity
    Restore from backup

Creating new identity...
Generated IID: k5xq7z4m2n3p5r6s7t2u3v4w5x2y3z7a

? Set admin password: ••••••••••••
? Confirm admin password: ••••••••••••

Identity created successfully!

Admin UI available at: http://localhost:8080
Your IID: k5xq7z4m2n3p5r6s7t2u3v4w5x2y3z7a

Press Ctrl+C to stop the node.
```

### Headless Setup

```bash
# Generate admin token
ADMIN_TOKEN=$(openssl rand -hex 32)
ADMIN_TOKEN_HASH=$(echo -n "$ADMIN_TOKEN" | sha256sum | cut -d' ' -f1)

# Create config with admin token
cat > config.toml << EOF
[admin]
token_hash = "$ADMIN_TOKEN_HASH"
EOF

# Start node
postnode start --config config.toml

# Save admin token securely!
echo "Admin token (save this!): $ADMIN_TOKEN"
```

## Backup and Recovery

### Backup Types

| Type | Contents | Use Case |
|------|----------|----------|
| **Full** | Everything | Complete restore |
| **Identity** | Keys + identity doc | Move to new node |
| **Data** | Messages + apps | Data preservation |

### Creating Backups

```bash
# Via CLI
postnode backup create --output backup.tar.gz.enc

# Includes:
# - Identity keys (encrypted)
# - Identity document
# - Contacts
# - Message history
# - App data
# - Configuration

# Encrypted with backup password (prompted)
```

### Restoring Backups

```bash
# To new/empty node
postnode backup restore --input backup.tar.gz.enc

# Enter backup password when prompted
# Node restarts with restored data
```

### Scheduled Backups

```toml
[backup]
enabled = true
schedule = "0 3 * * *"  # Daily at 3 AM
retention_days = 30
destination = "/backups"  # Local path
# Or remote:
# destination = "s3://bucket/backups"
```

## Security Hardening

### Checklist

- [ ] Change default admin password
- [ ] Enable TLS for admin UI
- [ ] Configure firewall rules
- [ ] Set up automatic updates
- [ ] Enable audit logging
- [ ] Configure IP allowlist (if remote access needed)
- [ ] Set up backup schedule
- [ ] Review installed apps and permissions

### TLS Configuration

```toml
[admin]
require_tls = true
tls_cert = "/etc/postnode/cert.pem"
tls_key = "/etc/postnode/key.pem"

# Generate self-signed cert (for local use)
# openssl req -x509 -newkey rsa:4096 -keyout key.pem -out cert.pem -days 365 -nodes

# Or use Let's Encrypt with certbot
```

### Audit Mode

```toml
[security]
audit_log_enabled = true
audit_log_path = "/var/log/postnode/audit.log"
audit_events = [
  "admin_login",
  "admin_action",
  "app_install",
  "app_uninstall",
  "permission_change",
  "key_rotation",
  "backup_create",
  "backup_restore"
]
```
