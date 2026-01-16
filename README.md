# img-server

一个基于 Rust (Axum) 构建的图片服务器。

## 特性

- 高性能 I/O: 下载接口采用 `Async Read -> Async Write` 流式传输，内存占用极低，支持大文件传输。
- CAS 存储: 基于 SHA256 内容寻址存储，自动去重（相同内容不同文件名的图片只存储一份物理文件）。
- 缩略图生成: 上传时自动生成缩略图。
- 安全机制:
  - 基于 CLI 生成的 Admin Token 鉴权（上传/删除）。
  - IP 黑名单机制。
  - 操作日志审计（记录 IP、操作类型）。

## 快速开始

### 1. 生成管理员 Token

首次使用前，需要生成一个 Admin Token 用于上传和删除操作。

```bash
./img-server gen-token
```

### 2. 启动服务器

默认监听 `0.0.0.0:3918`，使用默认配置文件路径。

```bash
./img-server serve
```

或者指定端口和配置文件：

```bash
./img-server --config ./my-config.toml serve --addr 127.0.0.1:8080
```

## 配置说明

配置文件默认位于 `~/.config/img-server/config.toml`。程序启动时会自动创建默认配置。

```toml
# 图片存储目录
images_dir = "data/images"
# 缩略图存储目录
thumbs_dir = "data/thumbs"
# 临时文件存储目录
temp_dir = "data/temp"
# 最大上传大小 (MB)
max_size_mb = 20
# 管理员 Token 列表 (通过 CLI gen-token 添加)
tokens = ["YOUR_ADMIN_TOKEN"]
# IP 黑名单
blacklist = ["192.168.1.100"]

# 图片元数据列表 (自动维护，请勿手动修改)
[[images]]
name = "example-image"
desc = "这是一个测试图片"
hash = "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
created_at = "2023-10-27T10:00:00Z"
```

## API 文档

### 1. 上传图片

- URL: `POST /images`
- 权限: 需要 Header `x-admin-token`
- Content-Type: `multipart/form-data`

| 字段   | 类型 | 说明         |
| :----- | :--- | :----------- |
| `name` | Text | 图片唯一名称 |
| `desc` | Text | 图片描述     |
| `file` | File | 图片文件     |

```bash
curl -X POST http://localhost:3918/images \
  -H "x-admin-token: YOUR_TOKEN" \
  -F "name=wallpaper" \
  -F "desc=My Wallpaper" \
  -F "file=@/path/to/image.jpg"
```

### 2. 列出图片

- URL: `GET /images`
- 权限: 公开

| 参数        | 说明     | 默认值 |
| :---------- | :------- | :----- |
| `page`      | 页码     | 1      |
| `page_size` | 每页数量 | 20     |

```bash
curl "http://localhost:3918/images?page=1&page_size=10"
```

### 3. 下载图片

支持通过图片名称或文件 Hash 下载。

- URL: `GET /images/:id`
- 权限: 公开

| 参数    | 说明                                            |
| :------ | :---------------------------------------------- |
| `:id`   | 图片名称 (name) 或 SHA256 Hash                  |
| `thumb` | 是否下载缩略图 (`true`/`false`)，默认为 `false` |

```bash
# 下载原图
curl -O -J http://localhost:3918/images/wallpaper

# 下载缩略图
curl -O -J "http://localhost:3918/images/wallpaper?thumb=true"

# 通过 Hash 下载
curl -O -J http://localhost:3918/images/e3b0c442...
```

### 4. 删除图片

- URL: `DELETE /images/:name`
- 权限: 需要 Header `x-admin-token`

```bash
curl -X DELETE http://localhost:3918/images/wallpaper \
  -H "x-admin-token: YOUR_TOKEN"
```

## 存储逻辑

1.  文件命名: 所有文件均以其内容的 SHA256 Hash 命名。
2.  去重: 如果上传两张内容相同但名称不同的图片，服务器只会存储一份物理文件，但在元数据中会有两条记录指向同一个 Hash。
3.  删除: 删除图片时，只有当没有任何元数据引用该 Hash 时，物理文件才会被删除。

## License

MIT
