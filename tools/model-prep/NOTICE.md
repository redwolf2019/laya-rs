# 来源与许可

本目录的 `export.py` 适配自 Receptron 的
[export_onnx.py](https://github.com/receptron/laya/blob/6478649e723122ca24bbf5fb69ed1010023c9750/export/export_onnx.py)。
改动：固定本地来源校验、动态 B/L/K 范围、fp32 检查、外部权重保存和独立验证。

官方 `rl_common.py` 从 `he-jev/laya@c5d78730f3493e4fe16d61507ef4b78eef7318cf`
下载，未修改；官方 README 声明 Apache-2.0，该 revision 没有独立 LICENSE 文件。
权重与 tokenizer 来自 `convaiinnovations/laya-multilingual@052592a15d198d9ad47da779604259b10b47b7aa`，
模型卡声明 Apache-2.0。准备脚本保留这两个原始声明和 Apache-2.0 标准文本；
标准文本来自 Apache 网站，不伪称为上游仓库自带文件。未添加上游未提供的版权人姓名。
这些文件随本地 bundle 的 `licenses/` 目录保留。后续分发仍须保留适用的许可和归属。

以下是所用 Receptron 导出器的完整许可：

MIT License

Copyright (c) 2026 Receptron

Permission is hereby granted, free of charge, to any person obtaining a copy
of this software and associated documentation files (the "Software"), to deal
in the Software without restriction, including without limitation the rights
to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
copies of the Software, and to permit persons to whom the Software is
furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in all
copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
SOFTWARE.
