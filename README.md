# img-server

English | [简体中文](./README_zh-CN.md)

A simple image server built with Rust Axum.

## Features

- High Performance I/O: Streaming `Async Read -> Async Write` for minimal memory usage, supporting large file uploads.
- CAS Storage: SHA256 Content-Addressable Storage with automatic deduplication (identical content shares one physical file).
- Thumbnails: Auto-generated upon upload.
- Security:
  - CLI-generated Admin Token authentication (for Upload/Delete).
  - IP Blacklisting.
  - Operation audit logging.

## Quick Start

### 1. Generate Admin Token

Generate a token for upload/delete operations before first use.

```bash
./img-server gen-token
```

### 2. Start Server

Listens on `0.0.0.0:3918` by default.

```bash
./img-server serve
```

Or specify port and config file:

```bash
./img-server --config ./my-config.toml serve --addr 127.0.0.1:8080
```

## Configuration

Default location: `~/.config/img-server/config.toml`.

```toml
# Storage paths
images_dir = "data/images"
thumbs_dir = "data/thumbs"
temp_dir = "data/temp"

# Max upload size (MB)
max_size_mb = 20

# Admin Tokens (Add via CLI `gen-token`)
tokens = ["YOUR_ADMIN_TOKEN"]

# IP Blacklist
blacklist = ["192.168.1.100"]

# Thumbnail size (pixels)
thumbnail_pixels = 50000

# Metadata (Managed automatically, do not edit)
[[images]]
name = "example"
# ...
```

## API Documentation

### 1. Upload Image

- URL: `POST /images`
- Auth: Header `x-admin-token`
- Type: `multipart/form-data`

| Field  | Description       |
| :----- | :---------------- |
| `name` | Unique image name |
| `desc` | Description       |
| `file` | Image file        |

```bash
curl -X POST http://localhost:3918/images \
  -H "x-admin-token: YOUR_TOKEN" \
  -F "name=wallpaper" \
  -F "desc=My Wallpaper" \
  -F "file=@/path/to/image.jpg"
```

### 2. List Images

- URL: `GET /images`
- Params: `page` (default 1), `page_size` (default 20)

```bash
curl "http://localhost:3918/images?page=1&page_size=10"
```

### 3. Download Image

- URL: `GET /images/:id`
- Params:
  - `:id`: Image name or SHA256 Hash.
  - `thumb`: `true`/`false` (default false).

```bash
# Download original
curl -O -J http://localhost:3918/images/wallpaper

# Download thumbnail
curl -O -J "http://localhost:3918/images/wallpaper?thumb=true"
```

### 4. Delete Image

- URL: `DELETE /images/:name`
- Auth: Header `x-admin-token`

```bash
curl -X DELETE http://localhost:3918/images/wallpaper \
  -H "x-admin-token: YOUR_TOKEN"
```

## Storage Logic

1.  Naming: Files are named using their SHA256 hash.
2.  Deduplication: Multiple uploads of identical content (with different names) are stored as a single physical file.
3.  Deletion: The physical file is only removed when no metadata records reference that hash.

## License

MIT
