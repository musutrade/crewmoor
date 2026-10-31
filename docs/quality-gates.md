# crewmoor quality gates

Rust 最新 stable、edition 2024，不维护旧版本/MSRV 兼容矩阵。根 `Cargo.toml` 是 CrewMoor 业务 crate；`tools/quality` 是 Rust 质量 CLI，`fixtures/quality` 是隔离验收 fixture。

## 本地与 CI 的统一入口

```sh
rustup update stable
rustup component add --toolchain stable clippy rustfmt llvm-tools-preview
cargo +stable install cargo-llvm-cov --version 0.9.0 --locked
CARGO_TARGET_DIR="$PWD/target" cargo +stable run --locked --manifest-path tools/quality/Cargo.toml -- verify
```

CI 最后一步使用相同 `verify` 命令，job 显式设置 `CARGO_TARGET_DIR=${{ github.workspace }}/target`。所有调度、配置检查、源码检查、CRAP 计算和验收测试均为 Rust；YAML 仅声明安装及 Cargo 调用。

并行开发/审计必须在各自调用中设置不同的绝对 `CARGO_TARGET_DIR`，并让完整 verify 的全部 Cargo/LLVM 子进程继承它；不要共享同项目不同 checkout 的 target，也不要修改全局 Cargo/代理配置或 clean 共享目录。报告记录准确 SHA、target、命令及日志；覆盖率源码映射出现另一 checkout 时必须失败，不能忽略 unknown coverage source。Issue #2 中旧共享 target 下的通过结论已撤回，不作为验收依据。

`verify` 依次检查工具自身、fixture，然后判断根目录业务 Cargo.toml：没有业务 crate 时明确输出 N/A；有业务 crate 时运行同一完整检查。工具自身也执行 fmt、策略检查、Clippy `--workspace --all-targets --all-features -- -D warnings`、真实测试覆盖率及全部命名函数的 CRAP，包括测试源码。每个子进程必须成功，不吞失败。

可独立运行以下子命令（相对于仓库根目录）：

```sh
cargo run --locked --manifest-path tools/quality/Cargo.toml -- lint-policy fixtures/quality/Cargo.toml
cargo run --locked --manifest-path tools/quality/Cargo.toml -- check fixtures/quality/Cargo.toml
cargo run --locked --manifest-path tools/quality/Cargo.toml -- crap fixtures/quality/Cargo.toml /absolute/path/coverage.json
```

`check` 会自行生成本次测试的真实覆盖率。`crap` 用于分析已经生成的 LLVM JSON，调用方负责报告与源码版本一致；CI 使用 `verify`，不接收外部覆盖率、阈值或 baseline 参数。直接检查缺失的业务 Cargo.toml 会失败，N/A 仅由 `verify` 的业务存在性判断明确输出。

## CRAP <= 10 的度量定义

公式：`CC² × (1 − coverage)³ + CC`。coverage 在 0–1 范围；10 通过，超过 10 失败，不按显示舍入值判定。

- 使用 `syn` AST 对自由函数、impl 方法、有默认体的 trait 方法逐个计算 CC，起点为 1。if、while、for、loop、?、&&、|| 各加 1；match 增加分支数减一及 guard 数。一个分支内的 or-pattern 不另加路径。闭包分支计入所属函数；嵌套命名函数另算，不重复计入父函数。
- 覆盖率为函数起止行范围内的 LLVM **可执行行命中比例**：segment 有 has_count、不是 gap 时纳入该行，同一行任一计数大于零即命中。这是明确的 source-line 模型，不是分支覆盖率，也不是“函数被调用过即 100%”。相比初始方案，迁移版改为该行模型，并将闭包控制流计入所属函数。
- `cargo llvm-cov --workspace --all-targets --all-features --json` 实际执行测试。加 `--no-default-ignore-filename-regex` 保留测试源码度量，`--include-build-script` 纳入 build.rs。`--no-rustc-wrapper` 避免注入额外编译 wrapper，仍保留本机已有 Cargo 配置。
- 空报告、无效 JSON/schema、非法坐标/计数、缺文件度量、函数没有可执行行及零函数项目都失败；实际有度量且未覆盖的行按零覆盖率处理。只有声明而无函数的文件无需覆盖率。
- 源清单从 Cargo metadata 的 workspace/local-path package target **入口文件**出发，解析普通模块、内联模块、字面量 `#[path]` 和 item 级 `include!` 引用，包含 target 目录之外的源文件；不会按 target 父目录递归把 tests 误归为生产。build.rs 作为独立入口及其模块分析。生产和测试共用文件按生产处理；解析到的条件编译源码无度量时失败。动态 include、非 item include、条件 path、缺失/歧义模块路径会明确失败。
- 覆盖率文件先规范化路径，再与源清单核对。只排除 metadata 明确标识的 registry/git 第三方依赖根和实际 rustc 的编译器源码根；其余未入 AST 清单的文件一律失败，不静默过滤。第三方源码不纳入项目策略检查。local-path dependency 视为自有源码；若其目标未被本次 workspace 测试覆盖，同样失败，应将其加入 workspace 测试范围。

模型边界：不实现完整宏展开。已知断言、格式化、vec、matches、env、json 等宏作为不透明调用，显式控制流 token 出现在参数中则拒绝；其他表达式宏拒绝。宏内部生成的控制流与函数未展开计数，不声称覆盖宏展开后的全部函数。条件编译矩阵、跨平台和 doctest 覆盖率尚未实现。函数行范围模型对同行函数/嵌套函数存在共享行度量；建议函数各自独立行，业务引入复杂生成代码前应扩展模型并独立审计。

实现参考 [herdr-flow-rust 的 Rust CLI](https://github.com/musutrade/herdr-flow-rust) 的 crap/lint-policy 分层及行覆盖率模型；未复制其 baseline、增量放行或缺失度量回退策略。

## unwrap 与 Clippy 零豁免

独立 Rust AST 规则禁止生产 `.unwrap()`、`Type::unwrap` 引用及非测试宏的 unwrap token；比较前统一去除 raw identifier 前缀，`r#unwrap` 不能绕过。`.expect()` 方法不在禁令内。

测试上下文只认明确的 `#[test]` 函数、`#[cfg(test)]` 项及其后代、文件级 `#![cfg(test)]`，或 Cargo metadata 的 integration-test target。不依赖文件名中的 test，也不因测试编译就把同文件生产函数豁免。模块解析会把 `#[cfg(test)] mod tests;` 的上下文传播给外置文件，不再要求额外文件级属性。测试限定的 item include 同样传播；复杂 cfg 表达式不推断为测试。

不启用 opt-in `clippy::unwrap_used`，不设置 Clippy 测试例外；测试 unwrap 仍须满足其他已启用 lint。所有 `allow`/`expect` lint 属性（包括 raw identifier、cfg_attr、测试与宏中的直接属性）均拒绝。Cargo lint allow/expect、Clippy 配置文件（包括目录祖先）、降低 lint 的 flags、未知编译 wrapper、相关环境覆盖，以及 CI 中常见 suppression/吞失败写法也拒绝。非 stable 的 RUSTUP_TOOLCHAIN 环境覆盖会失败。第三方依赖不在扫描范围。

这是防止项目常见误配置的规则，不能当成恶意代码沙箱：fragment 拼接生成的属性、宏 shadowing、第三方 proc-macro 展开以及修改检查器本身仍需要独立审计。保留已知 sccache 缓存 wrapper、普通链接参数和 LLVM 覆盖率 instrumentation 参数；不修改用户代理、Rustup/Cargo 配置或会话环境。

## 验证证据与限制

2026-10-31：`rustup update stable` 确认 stable 未变化；本机 `rustc 1.99.1 (58cf8f9c3 2026-10-13)`、`cargo 1.99.1 (5f94df478 2026-08-27)`、cargo-llvm-cov 0.9.0。CI 显式更新 stable；依赖保存在 Cargo.lock。

Rust 集成测试涵盖 CRAP 边界、无效/缺失度量、生产/测试/raw unwrap、raw suppression、外置测试模块约定、配置与命令失败传播。报告 schema 和调度失败用例使用合成数据/模拟 runner；完整 `verify` 对工具和 fixture 实际执行 fmt/Clippy/测试与覆盖率，二者证据不混称。

inventory 回归验证：外部 `#[path]`/include 文件中的 suppression、unwrap 被拒，外部 CC=4/零覆盖率的合成报告按 CRAP=20 拒绝，未知报告文件失败；build.rs 和包根 binary 不会改变 integration-test helper 的测试上下文。永久 fixture 同时包含外部模块、include 文件、build.rs、根级 binary 和 integration test，在完整 verify 中实际编译、运行并计量。

本机完整 `verify` 已通过。远端 GitHub Actions 尚未执行，当前业务验收 N/A，不以工具自测代替业务验收。

来源：[Rust 1.99.1 官方公告](https://blog.rust-lang.org/2026/10/15/Rust-1.99.1/)、[LLVM coverage export](https://llvm.org/docs/CommandGuide/llvm-cov.html#llvm-cov-export)、[Rust 模块与 path 属性规则](https://doc.rust-lang.org/reference/items/modules.html)。
