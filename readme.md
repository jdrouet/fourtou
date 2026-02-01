# Fourtou

A unified data store aggregator that connects multiple storage backends and exposes them through various protocols.

## Overview

Fourtou acts as a bridge between diverse data sources and access protocols. Define your storage backends (HTTP indexes, cloud drives, object storage) and expose them seamlessly through protocols like HTTP or Samba.

## Features (Planned)

### Sources
- **HTTP** - Public HTTP file indexes
- **Google Drive** - Google cloud storage
- **S3** - Amazon S3 or compatible storage
- **NFS** - Network File System mounts
- **pCloud** - pCloud cloud storage

### Exports
- **HTTP** - Serve files over HTTP with customizable paths
- **Samba** - Expose as SMB/CIFS shares
- **NFS** - Network File System exports

## Configuration

Fourtou uses TOML for configuration:

```toml
# Define a source
[[sources.ubuntu-images]]
type = "http"
base_url = "https://ubuntu.mirrors.ovh.net/ubuntu-releases/"

[[sources.family-pictures]]
type = "google-drive"
# configuration TBD

# Expose via HTTP
[[exports.public-http]]
type = "http"
socket = "0.0.0.0:4321"
prefix = "/public"
sources = [{ name = "ubuntu-images", alias = "ubuntu" }]

# Expose via Samba
[[exports.private-samba]]
type = "samba"

[[exports.private-samba.shares.family]]
source = "family-pictures"
```

## Built With

- [Rust](https://www.rust-lang.org/)

## Motivation

Self-host your data from multiple sources, whether they live on cloud providers, remote servers, or your local network. Fourtou unifies them into a single, coherent filesystem you can access from anywhere.

## Status

This project is in the early design phase. Contributions and ideas are welcome.

## License

TBD
