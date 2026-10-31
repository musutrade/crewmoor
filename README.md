# crewmoor

Rust 最新 stable、edition 2024；不维护旧版本/MSRV 兼容矩阵。目前没有业务 crate，`fixtures/quality` 只是隔离的门禁验收数据，`tools/quality` 是开发工具。

## 本地与 CI

需要 Python 3.11+、Rust stable 的 clippy/rustfmt/llvm-tools-preview 和 cargo-llvm-cov 0.9.0。依赖遵循现有 Rustup/Cargo 下载配置，不改代理。首次安装：

```sh
rustup update stable
rustup component add --toolchain stable clippy rustfmt llvm-tools-preview
cargo +stable install cargo-llvm-cov --version 0.9.0 --locked
python3 tools/quality/selftest.py
```

最后一条也是 CI 的验收命令：正例真实运行 Clippy、Rust 测试和 LLVM 覆盖率；负例在临时数据中验证规则，不把 suppression 加入实际项目代码。门禁工具自身执行策略检查与 `cargo clippy --all-targets -- -D warnings`。

业务 crate 出现后，提交其 Cargo.lock 并运行：

```sh
python3 tools/quality/gate.py Cargo.toml
```

该命令不接收跳过检查、放宽 lint 或自报覆盖率参数。它在同次运行生成新覆盖率报告；编译、测试、检查任一步失败都会返回非零。缺少 Cargo.toml、函数或度量也失败。CI 在没有根 Cargo.toml 时只报告「业务验收不适用」，不会把门禁自测成功说成业务通过；有根 Cargo.toml 时自动运行上述业务命令。

## 函数 CRAP <= 10

公式 `CC² × (1 − coverage)³ + CC`，coverage 在 0–1 范围；精确值 10 通过，大于 10 失败，不以打印舍入值判定。

- CC 来自 `syn` Rust AST：每个自由函数、impl 方法和有默认实现的 trait 方法独立从 1 开始；if、while、for、loop、?、&&、|| 各加 1；match 增加「分支数 − 1」及每个 guard，or-pattern 增加「备选数 − 1」。嵌套命名函数另算，不累加到外层。测试函数也计算 CRAP。
- 覆盖率来自 `cargo llvm-cov --workspace --all-targets --all-features --json` 实际测试运行后的 LLVM code regions（kind 0）命中数/总数，**不是**整个文件或「函数是否曾调用」的覆盖率。相同源位置的泛型实例区域按最大执行次数合并。按绝对文件路径、函数起始行与 AST 绑定；缺失、额外、不匹配、负计数、空度量均失败。
- 此 CC 是明确约定的语法圈复杂度模型，不声称与其他工具完全相同。无条件 loop 和 `?` 也计入，属于保守选择；覆盖率采用 region coverage，不是 line/branch coverage。
- 当前限制：闭包、自定义表达式宏、标准宏参数中的控制流不能可靠度量，拒绝而非记为 0。标准断言/格式化/vec/panic 类宏作为调用，不计算宏内部生成的分支。条件编译未进入本次 all-features 构建的源函数若无覆盖率会失败；需要另行设计配置矩阵后才能支持，不能静默忽略。相同行的多个函数也拒绝。宏生成函数若不能与 AST 对齐失败。尚不支持完整宏展开、跨平台矩阵、doc-test 覆盖率。
- 源文件清单来自 Cargo workspace 自有 target 的源目录递归扫描，包含测试；共享源文件按生产上下文处理。未编译的源文件不自动排除，缺度量时失败。工具与 fixture 的门禁自测不等于工具本身已经通过函数 CRAP 业务验收。

## unwrap 与 Clippy 零豁免

独立 AST 检查器禁止生产上下文中的 `.unwrap()`、`Type::unwrap` 引用及非测试宏中的 unwrap 标识符，不依赖文本文件名或当前 `cfg(test)` 编译标志。显式 `#[test]` 函数、`#[cfg(test)]` 项及其后代、文件级 `#![cfg(test)]`、Cargo metadata 指定的 integration-test target 属于测试上下文。生产函数即使与测试模块同文件仍检查。复杂 cfg 表达式不会推断为测试；可采用明确的 `#[cfg(test)]` 结构。宏 token 检查可能保守拒绝同名非调用标识符。

不启用 opt-in `clippy::unwrap_used`，也不设置 `allow-unwrap-in-tests`；测试 unwrap 仍须满足其他已启用 Clippy lint，例如不能使用会触发 unnecessary_literal_unwrap 的写法。Clippy 使用 `--workspace --all-targets --all-features -- -D warnings`。`.expect()` 方法没有被禁止。

自有 Rust 代码中的 `#[allow]` / `#[expect]`（包括 cfg_attr 嵌套及测试代码）拒绝；为避免分组 lint 间接绕过，当前保守禁止所有此类属性。Cargo lint allow、Clippy 配置文件、lint 降级 rustflags、未知编译 wrapper 和相关环境覆盖也拒绝。保留普通链接参数及已知的 `sccache` 缓存 wrapper，不修改用户配置。第三方依赖不属于策略扫描范围。

CI 工作流对 -A/--allow/--cap-lints、continue-on-error、常见吞失败 shell 写法做额外检查。项目执行路径固定为 Python checked subprocess，不提供自定义 shell hook。此检查防止常见配置误用，**不是**任意恶意修改检查器/CI 或外部编译器的安全沙箱；工作流及门禁自身变更仍需独立审计。

## 本次验证与来源

2026-10-31：`rustup update stable` 确认 stable 未变化；实际 `rustc 1.99.1 (58cf8f9c3 2026-10-13)`、`cargo 1.99.1 (5f94df478 2026-08-27)`、`cargo-llvm-cov 0.9.0`。CI 显式更新 stable，安装 coverage exporter 固定版本；Rust AST 依赖记录于工具 Cargo.lock。

官方来源：[Rust 1.99.1 公告](https://blog.rust-lang.org/2026/10/15/Rust-1.99.1/)、[LLVM coverage export](https://llvm.org/docs/CommandGuide/llvm-cov.html#llvm-cov-export)、[Clippy 配置说明](https://doc.rust-lang.org/stable/clippy/lint_configuration.html)。

已验证本机门禁正反例及边界。GitHub Actions 尚未远端运行；当前没有业务功能或业务覆盖率可验收。
