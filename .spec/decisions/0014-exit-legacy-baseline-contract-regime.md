# 0014 · 退出旧合同制：删复印件与 CI 校对、删 generated 树与 legacy_baseline，公共语义只从活契约取

- 日期:2026-09-06
- 状态:生效

## 背景

本仓长期同时挂着两套「公共语义来源」:

1. **旧合同制**——基线 `LGE-V1.4-2026-08-27` + 生成源仓 `LumioGameEngineArchitecture` + 复印到本仓的
   `crates/lumio-voxel-contracts/generated/` 只读产物树(420 KB,含 Root ABI bundle、`lumio_core.h`、
   C# 生成目录),由 `tools/architecture/generated-lock.json` 逐文件锁哈希、`check-generated-clean`
   在 CI 里校对,外加 `docs/architecture/` 六版架构镜像、`docs/plans/` 实现蓝图和 VOX-D 决策门。
2. **活契约**——架构仓 `engine/wire/voxel-world-v1.json`(`contractId: lumio.voxel-world.v1`),
   [0013](0013-voxel-world-contract-and-section-rename.md) 已让本仓消费它:`wire/` 逐字节副本 +
   `CONTRACT_SHA256` + 一致性测试。

第一套已经**没有上游**:生成源仓 `LumioGameEngineArchitecture` 不存在了,架构仓按 ADR-059 转入
Living Architecture,Baseline、`schemas/`、`tools/lumio_contract.py` 全部废止。一份永远不可能重新
生成的复印件,配上一套仍在 CI 里跑的校对,校对的是「复印件没被人手改过」——它证明不了任何与当前
公共语义有关的事情,却让仓里出现**第二份真值**:错误 id 有两套命名空间(活契约的 snake_case 与镜像的
`STABLE_ERROR_IDS`),16³ 数据单元有两个名字(契约的 Section 与镜像的 Chunk)。

## 决策

**整套旧合同制退出,不留兼容层、不留别名、不留兜底常量。** 分三层执行:

1. **文档 / CI / 镜像 / 旧模块图 / 空壳 crate**——删 `docs/architecture/`、
   `docs/LumioVoxelEngine_Framework_Design_LGE-V1.3/`、`docs/plans/`、`docs/evidence/decision-gates/`、
   `docs/evidence/v1.4-generated-artifact-gate.md`;`.github/workflows/repository-policy.yml` 不再 grep
   基线字符串、不再 `sha256sum -c`、不再跑 `check-generated-clean`;根 README 的「架构基线」「Architecture
   Gate」「Generated Contract Dependencies」三节合并成一节「公共契约来源」;`modules/` 按架构仓
   `features/voxel.md` 的 M1 ~ M10 模块图重写,只保留有代码的模块,删 `mesh-collision` / `migration` /
   `spatial` / `streaming` 四个纯文档骨架目录;删空壳 crate `lumio-voxel-migration`,workspace 从七个
   crate 变六个,DAG 守卫同步。
2. **`generated/` 树与它的配套**——删 `crates/lumio-voxel-contracts/generated/`、
   `tools/architecture/generated-lock.json`、`check_generated_clean.py`、
   `lumio_voxel_test_support::generated_clean` 与 `check-generated-clean` 例子、
   `crates/lumio-voxel-contracts/src/legacy_baseline.rs`、锁哈希校验测试,以及 VOX-D 决策门 benchmark
   与 V1.4 fixture 骨架。`lumio-voxel-contracts` 只剩 `voxel_world` 模块加它自己的水管
   (SHA-256、Hash 链、有界缓冲),水管在 crate 内自有实现,不再从被删的树里取。
3. **活代码**——不再使用 `Generated*` 类型、`BASELINE_ID`、`SCHEMA_EPOCH`、`STABLE_ERROR_IDS`、
   `P0_DECISION_GATES`、`from_generated`;错误 id 只剩活契约 `errorCodes` 那一套 snake_case。

退出之后,本仓只剩一种形状:**消费活契约的纯 Rust 体素 crate**。公共语义变更的唯一路径是
架构仓改 `engine/wire/voxel-world-v1.json` → 本仓同步 `wire/` 副本与常量 → 一致性测试变绿。

## 后果

- **接受**:随镜像一起消失的还有它提供的引擎通用错误 id 与 schema/binding 表。契约不定义的引擎通用
  失败(句柄、会话、预算、队列)需要本仓自己命名,或等架构仓在活契约里补;过渡期内相关断言会变薄。
- **接受**:`generated_clean` 守卫连同它「不复用被审计树内哈希器」的独立性论证([0010](0010-generated-clean-keeps-its-own-sha256.md))
  一并消失——被审计的对象没有了,守卫也就没有存在理由;仓里只剩一份 SHA-256。
- **接受**:`docs/architecture/` 六版镜像删除后,本仓不再能离线查阅历史架构文本;需要时去架构仓查。
- **换来**:仓里只有一份公共语义真值、一套错误 id 命名空间、一个 16³ 数据单元的名字,不再有永远无法
  重新生成的复印件和证明不了任何事的 CI 校对。
- 被本条取代:[0007](0007-v1.4-implementation-baseline.md)、[0009](0009-consume-adr-040-041-artifacts.md)、
  [0010](0010-generated-clean-keeps-its-own-sha256.md)。历史 ADR 正文不改写,只在状态行标注。
- **被本条部分取代:[0006](0006-crate-map.md) 的 crate 清单。** 空壳 crate `lumio-voxel-migration` 删除后
  workspace 从七个 crate 变六个,0006 表里的 `lumio-voxel-migration` 行与「`migration` 可晚于单域闭环
  启用」一句不再成立;0006 的其余部分——按 0003 分层合并 crate、逻辑模块不等于 crate 边界、五条约束、
  Foundation 最小集——一条都不撤。同样只在 0006 的状态行标注,正文不改写。
- **[0013](0013-voxel-world-contract-and-section-rename.md) 不在此列,它仍然生效。** 本条是它的延续:
  0013 定「体素公共语义改从活契约取、16³ 数据单元叫 Section」,本条只是把当时还留着的另一份来源
  (死基线镜像)整棵删掉,让 0013 定的那条唯一路径真正唯一。0013 定下的东西——`wire/` 逐字节副本、
  `CONTRACT_SHA256`、一致性测试、`ChunkPayload::schema_id()` 仍返回 `voxel-chunk-page`——一条都不撤。
