# laya-rs

Laya System-1 模型的 Rust 推理运行时与 HTTP 服务。在 Linux CPU 上运行模型，
根据文本或结构化信息进行分类、评分和命题判断，适用于工单分流、优先级评分、工具路由等场景。

默认模型为 `laya-multilingual`，支持中文、英文及模型支持的其他语言。
模型常驻内存，多个客户端通过 HTTP 共享推理资源。服务使用 ONNX Runtime，
运行时不需要 Python、Node.js、PyTorch 或 GPU。

## 能做什么

提交一份 `state` 和一组 `questions`，服务按问题名返回答案：

| 问题类型 | 用途示例 | 返回结果 |
| --- | --- | --- |
| `choice` 选择 | 工单应交给哪个部门？ | 选中的候选项及各项概率 |
| `score` 评分 | 处理优先级有多高？ | 从 0 开始的等级评分，可为小数，并附等级分布 |
| `noul` 命题概率 | 是否需要人工介入？ | 命题成立的概率，范围 0–1 |

答案来自预先定义的选项或等级，服务不生成聊天回复。

## Docker 部署（Linux ARM64 CPU）

需要 Docker 和固定版本的模型包。当前 Dockerfile 只支持 `linux/arm64`；
以下配置沿用已验收的 8 vCPU / 12 GiB 资源配额，Docker Desktop 需预留相应资源。
验收环境与性能数据见 [MVP 验收报告](docs/mvp-validation.md)。

### 1. 准备模型

**本项目目前没有可直接下载的模型包。** 请按[模型准备教程](tools/model-prep/README.md)
完成一次性导出与对照，或取得已准备好的模型包。导出需要独立的 Python/PyTorch 环境，
Docker 镜像构建和服务运行不需要它们。

在仓库根目录准备以下文件，内容须与[固定模型清单](docs/model-manifest.json)一致：

```text
models/multilingual/
├── laya.onnx
├── laya.onnx.data
├── laya_config.json
├── tokenizer/
│   ├── tokenizer.json
│   └── tokenizer_config.json
└── licenses/                 # 清单列出的全部许可文件
```

启动时会校验文件大小和 SHA-256，不接受其他版本的模型或配置。
容器用户 `65532:65532` 须能读取这些文件；运行期间保持模型目录不变。

### 2. 构建并启动

在仓库根目录执行。构建需要联网下载依赖，不会下载模型：

```sh
docker build --platform linux/arm64 -t laya-rs:local .
docker run -d --name laya-server --platform linux/arm64 \
  --cpus 8 --memory 12g --memory-swap 12g \
  --read-only --cap-drop ALL --security-opt no-new-privileges \
  --mount "type=bind,source=$PWD/models/multilingual,target=/models/multilingual,readonly" \
  -p 127.0.0.1:8080:8080 laya-rs:local \
  --model /models/multilingual --threads 1 --inter-op-threads 1 \
  --max-concurrency 1 --shutdown-grace 120
```

查看启动日志并检查就绪状态：

```sh
docker logs laya-server
curl -fsS http://127.0.0.1:8080/readyz
```

模型校验和预热完成前端口不会监听；就绪后返回 `{"status":"ready"}`。
若容器退出，先查看日志并核对模型文件、读取权限与内存配额。

停止服务时，给在途请求留出退出时间：

```sh
docker stop --timeout 130 laya-server
docker rm laya-server
```

`--timeout` 应大于服务的 `--shutdown-grace`（默认 120 秒）。

## 调用 API

服务启动后，发送一条包含三种问题的请求：

```sh
curl -fsS http://127.0.0.1:8080/v1/system-one \
  -H 'Content-Type: application/json' \
  --data '{
    "state": "客户说重复扣款，希望立即退款。",
    "questions": {
      "department": {
        "type": "choice",
        "instructions": "应该由哪个部门处理？",
        "criteria": {"billing": "支付、退款、账单", "technical": "技术问题", "sales": "销售问题"}
      },
      "priority": {
        "type": "score",
        "instructions": "这个请求的处理优先级如何？",
        "criteria": ["低", "中", "高", "紧急"]
      },
      "urgent": {
        "type": "noul",
        "instructions": "这个请求是否紧急？"
      }
    }
  }'
```

成功响应包含 `model`、`answers` 和 `usage`。读取本例结果时：

- `answers.department.choice`：选中的部门键；`probabilities` 为各部门概率。
- `answers.priority.score`：0–3 之间的评分，对应从“低”到“紧急”的有序等级。
- `answers.urgent.noul`：紧急这一命题成立的概率。

`usage.input_tokens` 为输入 token 数，`usage.output_tokens` 为 0。
完整响应字段、输入限制与错误码见 [API 契约](docs/compatibility.md)。

| 路由 | 用途 |
| --- | --- |
| `POST /v1/system-one` | 提交状态与问题，返回推理结果 |
| `GET /healthz` | 检查进程存活 |
| `GET /readyz` | 检查服务是否可接收推理请求 |
| `GET /metrics` | 获取 Prometheus/OpenMetrics 指标 |

## 构建与 CLI

如需从源码运行，在 Linux 上安装 Rustup、C/C++ 编译工具（`gcc g++ libc6-dev pkg-config`），
并准备模型包及 ONNX Runtime CPU 1.28.0 动态库。
仓库固定 Rust 1.98.1；原生库获取与校验见[运行环境说明](docs/validation-environment.md#官方-cpu-原生库)。

```sh
cargo build --release --locked
./target/release/laya-server --help
./target/release/laya-server --model ./models/multilingual \
  --ort-library /opt/onnxruntime/lib/libonnxruntime.so --threads 2 --max-concurrency 1
```

将 `--ort-library` 替换为本机动态库的实际路径。
全部参数与默认值见 `--help` 或 [CLI 契约](docs/compatibility.md#71-cli-与限制)。

## 更多文档

- [模型准备与对照](tools/model-prep/README.md)
- [完整 API 与运行配置](docs/compatibility.md)
- [Docker 部署细节与验收记录](docs/validation/docker.md)
- [MVP 验收范围与 CPU 性能数据](docs/mvp-validation.md)
- [服务设计方案](docs/laya-server-plan.md)

## License

项目原创代码使用 [MIT License](LICENSE)，移植代码归属见 [NOTICE](NOTICE.md)。
模型权重及第三方运行库遵循各自许可证。
