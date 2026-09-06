---
status: in_progress
---

# 运行期架构审查修复

## 范围

修复 World 固定容量、可观察性、发布 token 历史增长、锁内退休、generation 绕回，
避免不可变 sidecar 摘要反复计算并保持完整 Section 目录。
不改公共 wire、不伪造完整冷恢复、不新增失败后自动淘汰回执。

## 验收

真实 Router 连续 64 笔修改；旧读取不变；首笔重复返回原回执；显式小容量满载拒绝
不变更根且仍可重试；同 TxnId 不同请求拒绝；未参与修改的三种 presence 不消失；
摘要缓存对 Uniform/Palette/Raw 的字节语义不变；generation 耗尽不能重用。
显式查询预算与 pin 释放恢复；发布令牌重复消费/Clone 的编译失败测试。
现有 fmt/clippy/全工作区测试、依赖图及规范检查通过；新增三平台工作区测试。

## 证据与状态

测试先行提交 `14ef010d3625c0919ee241f688519e27482d333e` 的 Ubuntu Actions
job `101468206072` 实测 `transaction 17: BudgetExceeded`。
修复源码 `7f6a990649d4ce339dbd6bfa99b48735d130975c` 经 run `34027362886` 的
完整工作区验证通过。本任务实现已提交 PR #20，保留 in_progress 表示待维护者独立审查及合并，
不把 CI 绿色冒充独立审查通过。范围及未完成工作见
[交付记录](../../docs/reviews/2026-09-06-runtime-hardening.md)。
