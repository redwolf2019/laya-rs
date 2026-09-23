# 后处理验收（#13）

日期：2026-09-23。来源：[任务 #13](https://github.com/redwolf2019/laya-rs/issues/13)、
[兼容契约 §5/9](../compatibility.md)、[#10 固定 fixtures](../../tests/fixtures/README.md)。

`[已验证/HIGH，固定输出重放范围]` 全部 21 个官方模型请求的原始 logits 经 Rust 后处理，
完整响应与固定官方 API 精确相等；未舍入概率满足 `1e-5 + 1e-4*abs(reference)`。
对应 21 组 ONNX 输出也与同组 logits 的官方后处理 AST 重放结果精确相等。
这些测试不运行模型，不能作为实际 Rust 推理或完整模型兼容证据。

`[已验证/HIGH，跨后端失败]` 显式执行 `onnx_outputs_match_official_answers_exactly` 仍退出
101：`long-padding` 的 Score 为 ONNX/Rust `4.5306`，官方模型 `4.5307`。
见[失败日志](postprocess/cross-backend.log)，原始数据仍在 #10 fixtures。
同组 logits 的后处理对照通过，将当前失败定位到两组模型输出的差异；没有用业务再校准、
改期望或放宽容差消除它。#13 的跨后端四位字段验收未通过，任务保持开放。

## 实现与边界

[`Calibration`](../../src/postprocess.rs) 由加载器从原有配置字段反序列化，启动时校验全部
温度，包括未使用的桶。缺失字段沿用 `[1,1,1]` / `{}`；null、错形、非正和非有限值失败。
不改变 bundle manifest、配置文件或权重。新增依赖为零。

`response` 接收规范化 Request、按行连续的 float32 logits/act_probs 及实际 shape，
input_tokens 由调用方传入 Sequence Builder 的 usage。B 必须等于问题数，K 必须等于
批内最大真实选项数；action 为 `[B,2]`。检查缓冲区长度、维度转换与乘法、全部槽有限性；
padding 槽不参与概率。dtype 由 `&[f32]` 限定，未来 ORT 调用方须用 float32 提取接口。

主运算使用 f32，Score 加权和使用 f64；confidence 保留 NumPy 2.3.3 的 f32 标量运算。
每行先除选定温度再减最大值，拒绝产生非有限中间值的运算。
温度虽为有限正 f64，若无法作为有限正 f32 使用，返回数值错误，不改为默认值。
未舍入概率及 action 的范围、和按固定容差检查；各字段舍入后不二次归一化。
Choice 精确并列取首项，近并列按原始概率；Score 是期望等级；Noul 是第 1 项概率。
action 原样返回第 0 项，不温度缩放、不再次 softmax、不四位舍入。

四位舍入使用标准库定点格式化再解析，避免乘以 10000 的额外浮点舍入。
测试使用 #10 的 `0.03125` 中点及相邻 binary64 值；补充正负零、十进制中点样例来自
已有 Linux 开发容器的 CPython 3.11.16 `round(x,4)` 探针。
真实与人工输入在测试中明确区分，人工样例不证明模型校准质量。

人工覆盖三类型 k=2/3/5/6/10/11/32 桶覆盖/缺桶回退、巨大/近似 logits、精确/近并列、
零概率、均匀 32 项、四位舍入和不为 1、非饱和 action、坏 shape/索引、部分/全坏 logits、
坏 action、缩放/减法溢出和后续行失败的原子性。生产错误仅为静态枚举与文本。
测试差异定位仅打印 fixture 名、字段序号和数值差异，不打印业务文本。

## 复现

```sh
rtk cargo test --locked --test postprocess
rtk cargo fmt --check
rtk cargo clippy --locked --all-targets -- -D warnings
rtk cargo test --locked
rtk proxy docker start laya-loader-8
rtk proxy docker exec -w /work laya-loader-8 cargo test --locked --test postprocess
# 独立兼容门槛，当前明确失败；普通测试列为 ignored，不伪称通过。
rtk cargo test --locked --test postprocess onnx_outputs_match_official_answers_exactly -- --ignored --nocapture
```

`[已验证/HIGH]` 宿主 fmt、clippy 通过，完整测试为 44 passed / 5 ignored。
宿主与 Linux ARM64 后处理均为 9 passed / 1 ignored；跨后端门槛单独执行失败。
其余四个 ignored 为既有真实 tokenizer 专项和模型 smoke，本次未重跑。
本次未执行实时模型推理、HTTP、性能或 amd64 验收。

code-review 基点为 `e209c25e0a5c8e1b1d05e4db784eb96aa9cfd6a1`。
Standards：0 项发现，静态审查未见规范违反或需单列的代码异味。
Spec：0 项新增实现缺陷，1 项未完成验收（上述跨后端四位字段差异）；独立重跑后处理
为 9 passed / 1 ignored。该定位尚未证明模型输出差异的底层原因。
