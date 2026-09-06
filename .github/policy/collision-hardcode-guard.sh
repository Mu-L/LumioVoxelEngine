#!/usr/bin/env bash
#
# collision-hardcode-guard —— 契约 rule `query.collision-comes-from-material-table` 的静态守卫。
#
# ## 这道门为什么在
#
# 契约 `crates/lumio-voxel-contracts/wire/voxel-world-v1.json` 的 rule
# `query.collision-comes-from-material-table` 写着:
#
#     「阻挡与否只能查材质类表;物理实现不得自带一份碰撞行为分支」
#     onViolation: collision_behavior_not_from_material_table
#
# 它的 invalidCase 是 `physics_hardcodes_liquid_passability`,given 是
# 「物理实现里写死『BlockType 是水就不阻挡』」——**那是一段源码,不是一份触发载荷**。
# 不存在可以构造的入参能让 receiver 在运行时吐出 `collision_behavior_not_from_material_table`;
# 契约自己也把这条标了 `"validatorCheck": false`。R-00448 的实现方与审查方独立得出同一结论:
# 为它造一个运行时检测点,只会造出一个永不触发的死分支(本批次刚拆掉过两处那样的空转门)。
# Owner 2026-09-06 改判:这条规则可执行,只是**不在运行时**——做成静态 CI 守卫,就是本脚本。
#
# 守的是「不许退化」,不是修现有违例:今天 `physics_query.rs` 是合规的,
# 唯一把格子判成 Hit / Empty 的地方只按调用方注入的 `MaterialClassLookup` 分类。
# 同时 `collision_behavior_not_from_material_table` 全仓只剩错误码清单一处引用,
# 没有守卫会退化成死常量;本脚本就是它的第二处、也是唯一有牙的引用。
#
# ## 判据(禁止什么)
#
# 在**物理 / 查询实现面**(SCAN_PATHS)内,禁止出现「按具体 BlockType / 方块名判定」的分支。
# 分两组,因为大小写敏感性不同:
#
#   IDENTITY_PATTERNS(大小写敏感)——把一个块的**身份**与某个具体取值挂钩:
#     1. `.raw() ==/!= <非零字面量>`      把块身份与写死的编号比
#     2. `.raw() ==/!= <大写开头的名字>`  把块身份与具名常量比(invalidCase 的原形 `raw() == WATER_ID`)
#     3. `.raw() {`                       对块身份做 match 分派
#     4. `BlockType::new(` / `from_raw(`  在物理面里现造一个具体 BlockType(只可能是为了跟它比)
#     5. `BlockType::<全大写常量>`        点名一个具体方块的关联常量
#
#   VOCABULARY_PATTERNS_*(snake / SCREAMING 一组,CamelCase 一组)——把**具体方块名**写进引擎物理面。
#     引擎是通用的:水、岩浆这类具体材质名属于游戏的材质表,不属于引擎的碰撞代码。
#
# ## 豁免(air 哨兵)
#
# `raw() == 0` **不算违例**,且是唯一豁免。依据是契约 `blockId.resolution.builtInSentinels`:
# type 0 是 air,`"ordinaryMaterialAndTemplateLookup": false`——契约明文规定它**不走**普通材质
# 查表,所以物理面必须、也只能自己认出它。判据 1 用 `[1-9]|0[xXoObB]` 排除十进制 0 来实现这条豁免;
# 换言之豁免的是「与十进制字面量 0 比较」这一种写法,`raw() == 0x0` 或 `== BlockType::AIR` 会红——
# 那是刻意的:哨兵只留一种规范写法,才守得住。
#
# ## 这道门自己会不会空转
#
# 「命中为零就绿」的门,前提一旦失效就是一道安静放行的假门。所以下面三道自检先把每个前提证伪一次:
#   自检 1  pathspec 真的解析到了物理实现(不只是「非空」,还必须包含 ANCHOR 那个文件)。
#           物理实现被改名 / 挪走 → 红,逼人重新划定扫描面,而不是悄悄脱扫。
#   自检 2  每一条判据都能在阳性样本(fixtures/collision-hardcode-violations.txt)上命中。
#           某条正则被写坏、或 runner 的 ERE 与开发机行为不一致,那条判据当场红,而不是永远零命中。
#           这道自检不是形式主义:本守卫初版的词汇判据写成 `\bWATER\b`,自检 2 当场把它照红了——
#           `_` 是词字符,`\bWATER\b` 根本匹配不到 `WATER_ID`,而 `WATER_ID` 正是 invalidCase 的形状。
#   自检 3  豁免样本(fixtures/collision-hardcode-allowed.txt)不被任何判据命中。
#           判据收得过紧、误伤合规写法(尤其 air 哨兵)也当场红。
# 三道自检加上主扫描的 `git grep` 退出码显式分诊(0/1 之外一律当失败),构成失败关闭。
#
# ## 已知边界(别误以为它管到了)
#
# - `git grep` 只看**被 git 跟踪的**文件。本地新建、尚未 `git add` 的物理实现文件不会被扫到;
#   CI 上 `actions/checkout` 拿到的是提交后的树,PR 里的新文件都已跟踪,故该缺口不影响门禁效力。
# - 判据是词法的,不是语义的。`match` 到别处再判、把编号先存进变量再比,这类绕法它抓不住;
#   它抓的是**退化最常走的那条路**——顺手在物理面写一个具体方块的 if。语义级约束仍靠评审。
#
# 本地跑法(与 CI 完全同一条代码路径):
#     bash .github/policy/collision-hardcode-guard.sh
#
set -euo pipefail

cd "$(git rev-parse --show-toplevel)"

# 物理 / 查询实现面。git pathspec 的 `*` 跨 `/`,所以 `crates/*/src/*physics*` 能罩住
# 任意 crate 下任意带 physics 的实现文件;新增同类文件自动进扫描面,不必改本脚本。
SCAN_PATHS=(
  'crates/*/src/*physics*'
  'crates/*/src/*collision*'
  'crates/*/src/*raycast*'
  'crates/*/src/*sweep*'
  'crates/*/src/query/*'
)

# 扫描面必须包含的锚点:唯一把格子判成 Hit / Empty 的实现。
ANCHOR='crates/lumio-voxel-project/src/physics_query.rs'

FIXTURE_POSITIVE='.github/policy/fixtures/collision-hardcode-violations.txt'
FIXTURE_EXEMPT='.github/policy/fixtures/collision-hardcode-allowed.txt'

IDENTITY_PATTERNS=(
  '\.raw\(\)[[:space:]]*[=!]=[[:space:]]*([1-9]|0[xXoObB])'
  '\.raw\(\)[[:space:]]*[=!]=[[:space:]]*[A-Z]'
  '\.raw\(\)[[:space:]]*\{'
  'BlockType::(new|from_raw|from_bits)\('
  'BlockType::[A-Z_][A-Z_0-9]*'
)

# 词汇判据分两条,因为方块名在 Rust 里有两种写法,而且**不能用 `\b`**:
# `_` 是词字符,所以 `\bWATER\b` 匹配不到 `WATER_ID` —— 恰恰就是 invalidCase 的那个形状。
# 改用「前后不是字母」自己划边界:
#   VOCABULARY_PATTERNS_ANYCASE(大小写不敏感)罩 snake_case 与 SCREAMING_CASE:
#     WATER_ID / is_water / magma_is_passable。后置 `[^A-Za-z]|$` 顺带排掉 watermark 这类
#     正当英文词(high/low watermark 在队列代码里是术语)。
#   VOCABULARY_PATTERNS_CASED(大小写敏感)罩 CamelCase 拼接:WaterBlock / LavaFlow;
#     后置要求大写或数字或 `_`,所以同样不碰 Watermark。
VOCABULARY_PATTERNS_ANYCASE=(
  '(^|[^A-Za-z])(water|lava|magma|obsidian|bedrock|cobweb|slime|ladder|gravel)([^A-Za-z]|$)'
)
VOCABULARY_PATTERNS_CASED=(
  '(^|[^A-Za-z])(Water|Lava|Magma|Obsidian|Bedrock|Cobweb|Slime|Ladder|Gravel)[A-Z0-9_]'
)

fail() {
  echo "::error::$*" >&2
  exit 1
}

# 判据数组展开成 `-e <pattern>` 参数(不用 mapfile:那是 bash 4+,本脚本要能在开发机
# 的 bash 3.2 上跑出与 CI 完全一致的结果,反例探针才有意义)。
IDENTITY_ARGS=()
for pattern in "${IDENTITY_PATTERNS[@]}"; do
  IDENTITY_ARGS+=(-e "$pattern")
done
VOCABULARY_ANYCASE_ARGS=()
for pattern in "${VOCABULARY_PATTERNS_ANYCASE[@]}"; do
  VOCABULARY_ANYCASE_ARGS+=(-e "$pattern")
done
VOCABULARY_CASED_ARGS=()
for pattern in "${VOCABULARY_PATTERNS_CASED[@]}"; do
  VOCABULARY_CASED_ARGS+=(-e "$pattern")
done

# --- 自检 0:阳性 / 豁免样本在场 ---------------------------------------------
[ -f "$FIXTURE_POSITIVE" ] || fail "阳性样本 $FIXTURE_POSITIVE 缺失,判据无法自证有效"
[ -f "$FIXTURE_EXEMPT" ] || fail "豁免样本 $FIXTURE_EXEMPT 缺失,豁免无法自证不误伤"

# --- 自检 1:pathspec 真的解析到了物理实现 -----------------------------------
matched="$(git grep -l -e 'fn ' -- "${SCAN_PATHS[@]}" || true)"
if [ -z "$matched" ]; then
  fail "扫描 pathspec 匹配不到任何源文件,这道门是空转的:${SCAN_PATHS[*]}"
fi
if ! printf '%s\n' "$matched" | grep -qxF "$ANCHOR"; then
  fail "锚点 $ANCHOR 不在扫描面内(被改名 / 挪走 / pathspec 写坏)。物理实现脱扫 = 这道门形同虚设,请先修正 SCAN_PATHS / ANCHOR"
fi

# --- 自检 2:每条判据都在阳性样本上命中 --------------------------------------
for pattern in "${IDENTITY_PATTERNS[@]}"; do
  git grep -qE -e "$pattern" -- "$FIXTURE_POSITIVE" \
    || fail "判据失效(在阳性样本上零命中,该正则形同不存在):$pattern"
done
for pattern in "${VOCABULARY_PATTERNS_ANYCASE[@]}"; do
  git grep -qiE -e "$pattern" -- "$FIXTURE_POSITIVE" \
    || fail "判据失效(在阳性样本上零命中,该正则形同不存在):$pattern"
done
for pattern in "${VOCABULARY_PATTERNS_CASED[@]}"; do
  git grep -qE -e "$pattern" -- "$FIXTURE_POSITIVE" \
    || fail "判据失效(在阳性样本上零命中,该正则形同不存在):$pattern"
done

# --- 自检 3:豁免样本不被误伤 ------------------------------------------------
set +e
exempt_hits="$(git grep -nE "${IDENTITY_ARGS[@]}" -- "$FIXTURE_EXEMPT")"
exempt_status=$?
set -e
case "$exempt_status" in
  1) : ;;
  0) fail "判据误伤豁免样本(air 哨兵等合规写法被判成违例):${exempt_hits}" ;;
  *) fail "豁免自检的 git grep 异常退出 $exempt_status" ;;
esac
set +e
exempt_hits="$(git grep -niE "${VOCABULARY_ANYCASE_ARGS[@]}" -- "$FIXTURE_EXEMPT")"
exempt_status=$?
set -e
case "$exempt_status" in
  1) : ;;
  0) fail "词汇判据(大小写不敏感)误伤豁免样本:${exempt_hits}" ;;
  *) fail "豁免自检的 git grep 异常退出 $exempt_status" ;;
esac
set +e
exempt_hits="$(git grep -nE "${VOCABULARY_CASED_ARGS[@]}" -- "$FIXTURE_EXEMPT")"
exempt_status=$?
set -e
case "$exempt_status" in
  1) : ;;
  0) fail "词汇判据(CamelCase)误伤豁免样本:${exempt_hits}" ;;
  *) fail "豁免自检的 git grep 异常退出 $exempt_status" ;;
esac

# --- 主扫描 -----------------------------------------------------------------
violation=0

set +e
git grep -nE "${IDENTITY_ARGS[@]}" -- "${SCAN_PATHS[@]}"
status=$?
set -e
case "$status" in
  0) echo "::error::物理 / 查询实现里出现按具体 BlockType 身份判定的分支(见上方命中行)。阻挡与否只能来自注入的材质类表,见契约 rule query.collision-comes-from-material-table / 错误码 collision_behavior_not_from_material_table;air 哨兵只有 \`raw() == 0\` 一种写法被豁免" >&2; violation=1 ;;
  1) : ;;
  *) fail "git grep(身份判据)异常退出 $status" ;;
esac

VOCAB_MESSAGE='物理 / 查询实现里出现具体方块名(见上方命中行)。引擎不认识水和岩浆,材质语义只能来自游戏侧的材质类表,见契约 rule query.collision-comes-from-material-table'

set +e
git grep -niE "${VOCABULARY_ANYCASE_ARGS[@]}" -- "${SCAN_PATHS[@]}"
status=$?
set -e
case "$status" in
  0) echo "::error::${VOCAB_MESSAGE}" >&2; violation=1 ;;
  1) : ;;
  *) fail "git grep(词汇判据 · 大小写不敏感)异常退出 $status" ;;
esac

set +e
git grep -nE "${VOCABULARY_CASED_ARGS[@]}" -- "${SCAN_PATHS[@]}"
status=$?
set -e
case "$status" in
  0) echo "::error::${VOCAB_MESSAGE}" >&2; violation=1 ;;
  1) : ;;
  *) fail "git grep(词汇判据 · CamelCase)异常退出 $status" ;;
esac

[ "$violation" -eq 0 ] || exit 1

echo "collision behavior comes only from the material-class table across: ${SCAN_PATHS[*]}"
