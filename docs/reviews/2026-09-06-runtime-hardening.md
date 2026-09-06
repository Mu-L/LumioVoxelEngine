# 2026-09-06 运行期修复范围与验收

基线：`d5efaf581e5dbb8f985bcd35f7fdc612e24096ef`；PR：#20。

## 本次改动

| 面 | 实施 | 证明范围 |
|---|---|---|
| World 预算 | 宿主注入 pin / receipt / query 上限与只读占用量 | 默认世界 64 笔真实修改；显式小容量拒绝与重试 |
| 发布生命周期 | 移除无界 token 历史；锁外销毁旧 root/pin | 一次性 seal、跨世界、并发原子性及不可复制/重复消费测试 |
| Generation | checked 原子递增，终止值不能绕回 | 耗尽永久拒绝、并发唯一性 |
| 摘要热路径 | 缓存已验证的不可变 sidecar 摘要 | Uniform、Palette、Raw、COW 字节语义不变 |
| 目录 overlay | 基于完整目录 COW，只更新变化项 | 无 revision 的 Pending/Unchanged/Unavailable 条目不丢失 |
| 平台验证 | Linux、Windows、macOS 工作区编译/测试 | 不等于 Android/iOS、真机或性能认证 |
| Agent 导航 | 更正 M6a 状态、明确 snapshot 范围；高风险语义不按行数豁免 | 不重新创建契约制度或空模块 |

默认开发预算为 64 个同时固定的 Revision、4096 项保留回执、256 个查询 Section。
这些只是可注入的实现策略，不是冻结公共限值。固定查询预算的差分夹具显式传入其
原有 `QUERY_BUDGET = 16`，参考算法、验证断言和 Golden 均保持不变。

## 不能标为已解决的工作

1. 回执安全回收。默认容量增大和可配置只是修复固定 16 项限制，不是无限持续运行保证。
   任意 TxnId 不能通过 FIFO/TTL 遗忘；必须先与 Runtime/Host 确立耐久确认及安全重放边界。
2. 完整冷恢复。当前 Capture 序列化元数据，RestoreShadowBuilder 生成 Unchanged 条目，
   修改后的完整 Section 页、基础资产、事务证据和新实例身份的恢复链路尚未闭合。
3. 发布权限封装。公开 publication_authority 仍被现有消费者与 fixtures 使用，迁移需要
   有类型的装载/副本/恢复入口；本次不加“测试特权”去伪装封装完成。
4. 全局目录/根指纹成本。减少重复 sidecar 编码，不等于消除 BTreeMap COW 或 Debug 指纹。
   更换规范摘要必须显式版本化并处理消费者，不静默改写旧 identity。
5. Streaming loader、eviction、mesh/light、真实性能和移动设备支持仍在后续范围。

## 验证记录

测试先行提交 `14ef010d3625c0919ee241f688519e27482d333e` 的真实 Linux 测试编译成功，
在 `runtime_limits` 用例第 17 笔事务返回 `BudgetExceeded`。这不是 mock 容量测试。

修复源码提交 `7f6a990649d4ce339dbd6bfa99b48735d130975c` 由 Actions run
`34027362886` / job `101470611141` 验证并提交。该 job 完成以下命令且全部 exit 0：

```sh
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo check --workspace --no-default-features --locked
cargo test --workspace --all-features --locked --no-fail-fast
cargo run -p lumio-voxel-test-support --example check-crate-dag
node .spec/tools/spec-lint.mjs
node --test .spec/tools/spec-lint.test.mjs
bash .github/policy/collision-hardcode-guard.sh
```

编辑环境没有 Rust 工具链，以上是 GitHub Actions 的真实结果，不是本地执行声称。
临时验证脚本不属于最终交付；最终源代码不依赖它们，仍使用普通 Cargo 命令。
最终三平台与仓库策略结果以 PR 对应精确 head SHA 的 Actions 为准，不能把旧提交的绿色
结果当成本次通过。上游架构契约缺席时的比对跳过仍是既有验证边界，本 PR 未修改契约。

本轮审查采用同上下文逐文件复核及现有独立参考实现的差分测试，没有声称存在另一位
独立 Reviewer；合并前仍应由维护者检查完整 PR diff。

## 宿主接入

```rust,ignore
let limits = WorldLimits {
    max_pinned_revisions: 32,
    max_receipts: 8192,
    max_query_sections: 128,
};
let world = VoxelWorld::create_with_limits(descriptor, approved_snapshot, limits)?;
let pressure = world.retained_receipt_count();
```

这是预算注入示意；不要把示例值当作设备性能承诺。满载行为保持 fail-closed，
不能通过增大到 usize::MAX 或后台丢弃旧回执掩盖持续运行需求。
