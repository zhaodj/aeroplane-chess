# 飞行棋项目技术方案（Bevy）

## 1. 文档目标

本文档基于 [requirement.md](/Users/zhaodaojun/Documents/studio/404man/code/aeroplane-chess/requirement.md) 的业务需求，输出一套适用于单机飞行棋项目的技术落地方案，目标是：

- 使用 Bevy 最新稳定版本进行开发
- 优先支持 2D 单机玩法与本地多人
- 先完成可玩的 MVP，再逐步扩展技能、事件点、难度 AI 和多平台适配

本文档面向首个可运行版本，不覆盖联网架构。

## 2. 技术目标与范围

### 2.1 首期目标

- 完成标准飞行棋核心循环：掷骰、出发、移动、飞跃、撞击、回家、终点判定
- 支持 `1v1` 与 `2v2` 本地模式
- 支持人机对战与本地双人协作
- 支持基础技能系统
- 支持简单/普通/困难三档 AI
- 覆盖 `PC + Web` 首发，移动端作为第二阶段适配

### 2.2 非首期目标

- 不做联网对战
- 不做账号、排行榜、每日任务
- 不做复杂小游戏事件
- 不做 3D 表现

## 3. 技术选型

### 3.1 引擎与语言

- 语言：Rust stable
- 引擎：Bevy `0.18.x` 目标线
- 渲染：Bevy 2D
- UI：Bevy UI
- 音频：Bevy 内建音频能力

说明：

- 当前公开索引中，`docs.rs` 的 `bevy/latest` 指向 `0.18.1`；GitHub Releases 搜索结果存在缓存滞后，仍显示 `0.17.3`。因此方案按 `0.18.x` 设计，实际初始化时以 `Cargo` 能解析到的最新稳定 `bevy` 版本为准。
- Bevy 版本迭代快且存在破坏性升级，项目初期必须锁定 minor 版本，避免开发中途 API 漂移。

### 3.2 建议依赖

首期尽量精简，只引入必要依赖：

- `bevy`：主引擎
- `rand`：骰子、AI 权重与事件随机
- `serde`：棋盘、技能、配置数据反序列化
- `ron` 或 `serde_json`：静态配置文件
- `thiserror`：规则层错误定义

可选依赖：

- `bevy_kira_audio`：若后续需要更强音频控制
- `leafwing_input_manager`：若后续输入映射复杂化
- `bevy_asset_loader`：若资源加载流程需要统一入口

首期不建议引入过多第三方插件，优先保证规则层稳定。

## 4. 总体架构

项目采用 Bevy 常见的 `AppState + Plugin + ECS + 配置驱动` 结构。

### 4.1 分层原则

- `domain`：纯规则层，不依赖 Bevy 渲染 API
- `application`：回合驱动、AI 调度、技能执行、事件编排
- `presentation`：棋盘表现、UI、动画、音频、输入
- `infrastructure`：资源加载、配置解析、存档、日志

这样做的目标：

- 规则逻辑可单测
- AI 决策可以脱离渲染进行模拟
- 后续从 PC 扩展到 Web/移动端时，核心逻辑无需重写

### 4.2 推荐目录结构

```text
src/
  main.rs
  app.rs
  states.rs
  constants.rs
  plugins/
    mod.rs
    boot_plugin.rs
    menu_plugin.rs
    game_plugin.rs
    board_plugin.rs
    piece_plugin.rs
    turn_plugin.rs
    skill_plugin.rs
    ai_plugin.rs
    ui_plugin.rs
    audio_plugin.rs
    animation_plugin.rs
  domain/
    mod.rs
    board.rs
    tile.rs
    piece.rs
    player.rs
    team.rs
    dice.rs
    rules.rs
    skill.rs
    event.rs
    victory.rs
  gameplay/
    mod.rs
    commands.rs
    reducers.rs
    turn_flow.rs
    action_resolver.rs
    ai.rs
  data/
    mod.rs
    board_config.rs
    skill_config.rs
    game_mode.rs
  ui/
    mod.rs
    menu.rs
    hud.rs
    overlays.rs
    result.rs
  assets/
    mod.rs
    handles.rs
```

## 5. 状态机设计

### 5.1 AppState

```rust
enum AppState {
    Boot,
    MainMenu,
    ModeSelect,
    CharacterSelect,
    SkillSelect,
    LoadingGame,
    InGame,
    Result,
}
```

### 5.2 InGame 子状态

```rust
enum GamePhase {
    RoundStart,
    AwaitDice,
    DiceRolling,
    AwaitPieceSelect,
    PieceMoving,
    ResolveTileEffect,
    ResolveSkillEffect,
    ResolveCombat,
    CheckVictory,
    RoundEnd,
}
```

设计原则：

- 一次系统只负责一个明确阶段
- 回合推进通过事件或 NextState 驱动
- 动画完成后再进入规则结算，避免表现与逻辑耦合混乱

## 6. 核心 ECS 建模

### 6.1 关键实体

- 棋盘格实体 `BoardTile`
- 棋子实体 `Piece`
- 玩家实体 `Player`
- 队伍实体 `Team`
- 骰子表现实体 `DiceView`
- UI 实体 `HudRoot / SkillButton / DiceButton / PromptPanel`

### 6.2 关键组件

```rust
struct PieceId(u8);
struct OwnerPlayerId(u8);
struct TeamId(u8);

// 相对本方起点的进度；换位到起点前两格时可为 -2/-1。
type PieceProgress = i16;

struct PieceState {
    status: PieceStatus,
    progress: PieceProgress,
    lap: u8,
    shield: u8,
    stacked_with: Option<Entity>,
}

enum PieceStatus {
    InHangar,
    Active,
    Finished,
}

struct TileIndex(u8);
struct WorldPosition(Vec2);
struct Selectable;
struct HumanControlled;
struct AiControlled;
```

### 6.3 关键资源

```rust
struct MatchConfig {
    mode: GameMode,
    ai_difficulty: AiDifficulty,
    fast_mode: bool,
}

struct TurnState {
    current_player: u8,
    extra_roll: bool,
    dice_value: Option<u8>,
    turn_index: u32,
}

struct BoardGraph {
    main_route: Vec<TileRef>,
    home_routes: [Vec<TileRef>; 4],
}

struct SkillInventory {
    charges: Vec<SkillCharge>,
}
```

原则：

- 棋子真实位置以“逻辑路径索引”表达
- 屏幕坐标只作为表现层缓存
- 不直接把游戏规则绑定到 Transform

## 7. 棋盘与规则建模

### 7.1 棋盘数据

棋盘使用静态配置驱动，不把格子逻辑硬编码在系统里。

建议配置字段：

```rust
struct TileConfig {
    id: String,
    kind: TileKind,
    screen_pos: Vec2Def,
    route_index: Option<u8>,
    jump_to: Option<u8>,
}
```

`TileKind`：

- `Normal`
- `Launch`
- `Jump`
- `Attack`
- `Defense`
- `Event`
- `HomeLane`
- `Goal`

### 7.2 经典规则拆分

建议把规则拆成纯函数：

- `can_launch(piece, dice) -> bool`
- `reachable_tiles(piece, dice, board) -> MovePlan`
- `resolve_jump(piece, tile) -> Option<JumpResult>`
- `resolve_collision(attacker, defender) -> CollisionResult`
- `resolve_stack(piece, teammate) -> StackResult`
- `check_finish(piece) -> bool`

这样便于：

- UI 高亮可移动棋子
- AI 评估所有可行动作
- 单元测试覆盖规则边界

### 7.3 队友叠加规则建议

需求中“叠加后免疫被撞一次”可以建模为：

- 同队棋子落在同一逻辑格时自动形成 `stack`
- `stack` 提供 1 层共享护盾
- 首次被敌方撞击时只消耗护盾，不回家
- 护盾消耗后 stack 解除或保留，需要在设计稿阶段确认

首期建议：

- 叠加后仍保持 stack
- 仅消耗一次 `shared_shield`
- 第二次再被撞时正常判定

这比“被打一次后立即拆分并回家”更直观，也更符合“协作防守”的需求表达。

## 8. 技能系统方案

### 8.1 技能建模

技能不要直接写成 UI 按钮逻辑，应该抽象成数据 + 执行器：

```rust
enum SkillId {
    Dash,
    Shield,
    Snipe,
    Swap,
    DoubleDice,
}

struct SkillDefinition {
    id: SkillId,
    max_charges: u8,
    target_rule: TargetRule,
    timing: SkillTiming,
}
```

### 8.2 技能执行时机

- `Dash`：掷骰后、移动前
- `Shield`：主动释放或受击前触发
- `Snipe`：回合行动阶段主动使用
- `Swap`：己方投骰前选择双方；任一方的 `PieceMoveAnimation` 尚存在（含延迟/停顿）时保留预览并禁用确认。UI 恢复可确认后也必须经共享执行入口复核，阻塞时不扣费、不消耗技能机会；AI 使用同一检查。
- `DoubleDice`：投骰前声明

### 8.3 技能执行流程

换位动画与后续移动共用本方重算进度对应的棋盘坐标；普通移动的起点/终点不从旧插值 Transform 推断。动画途中发生新的逻辑状态变化时，采样旧轨迹的当前可见位置接续新动画，替换旧动画组件，并将缓存终点同步到新逻辑落点，避免旧轨迹回写覆盖新状态；升空的临时 Z 偏移不写入逻辑落点。

1. 校验时机是否合法
2. 校验目标是否合法
3. 生成 `GameAction`
4. 由统一 `ActionResolver` 执行
5. 派发表现事件给动画/UI/音频

这样可以避免技能系统成为特判堆积点。

## 9. 回合系统设计

### 9.1 单回合流程

```text
进入玩家回合
-> 判断是否可使用前置技能
-> 投骰
-> 计算可行动棋子
-> 玩家或AI选择动作
-> 播放移动动画
-> 结算飞跃/撞击/防御/事件
-> 判断是否额外投骰
-> 判断胜负
-> 切换下一玩家
```

### 9.2 命令式动作抽象

推荐所有行动统一映射为命令：

```rust
enum GameAction {
    RollDice,
    LaunchPiece { piece: Entity },
    MovePiece { piece: Entity, steps: u8 },
    UseSkill { player: Entity, skill: SkillId, target: SkillTarget },
    EndTurn,
}
```

好处：

- AI 与玩家输入可共用同一执行通路
- 便于录像、重放与回放调试
- 后续联网时也能复用动作协议

## 10. AI 技术方案

### 10.1 设计原则

- 不做搜索树型复杂 AI，首期以规则权重 AI 为主
- AI 必须基于纯逻辑快照决策，不读取 UI 状态

### 10.2 决策流程

1. 收集当前玩家所有合法动作
2. 对每个动作做启发式评分
3. 叠加难度系数和随机扰动
4. 选出得分最高动作

### 10.3 评分维度建议

- 能否撞击敌方
- 能否保护己方关键棋子
- 能否进入终点
- 能否形成叠加
- 是否会把高价值棋子暴露在危险位置
- 技能使用收益
- 是否获得额外掷骰机会

### 10.4 难度分层

- 简单：随机合法动作 + 极少量规则过滤
- 普通：按基础收益打分
- 困难：增加风险预估、队友协同、技能保留策略

### 10.5 AI 实现建议

- `AiPlanner` 生成候选动作
- `AiEvaluator` 负责评分
- `AiPolicy` 负责按难度选择最终动作

不要把三个职责写进同一个系统里，否则后期难以维护。

## 11. UI/UX 方案

### 11.1 界面划分

- 主菜单
- 模式选择
- 技能选择
- 对局 HUD
- 结算页

### 11.2 对局 HUD 必备元素

- 当前玩家/队伍提示
- 骰子按钮与骰子结果
- 技能按钮与剩余次数
- 可行动棋子高亮
- 事件/战斗提示条
- 快速模式开关

### 11.3 交互原则

- 玩家无需理解内部规则计算
- 所有合法行动都要高亮
- 所有非法点击给出轻提示，不弹阻断框
- 动画允许跳过或加速

## 12. 动画与表现

首期不需要复杂骨骼动画，采用棋子位移 + 缩放/闪烁即可。

### 12.1 动画拆分

- 掷骰动画
- 棋子逐格移动
- 飞跃动画
- 撞击动画
- 护盾触发动画
- 到达终点动画

### 12.2 实现建议

- 规则层先生成移动路径
- 表现层按路径播放 Tween 风格插值
- 动画结束发出 `AnimationFinished` 事件
- 规则只等待事件，不直接操作视觉时长

## 13. 音频方案

首期资源最少化：

- `dice_roll`
- `piece_move`
- `jump`
- `hit`
- `shield`
- `win`
- `lose`

建议音频播放由统一 `AudioEvent` 驱动，避免业务系统直接到处播放声音。

## 14. 数据驱动设计

建议下列内容走配置：

- 棋盘格布局
- 格子类型
- 技能定义
- AI 参数权重
- 动画速度
- 局内文本

建议目录：

```text
assets/config/
  board_default.ron
  skills.ron
  ai_easy.ron
  ai_normal.ron
  ai_hard.ron
```

这样可以在不改代码的前提下调平衡。

## 15. 测试策略

### 15.1 优先测试层

- 规则纯函数单测
- 回合推进集成测试
- AI 打分器测试

### 15.2 重点测试用例

- 掷到 6 时起飞与额外回合
- 撞击时护盾消耗逻辑
- 队友叠加后再被撞
- 飞跃点连续触发
- 终点精确完成与支路超点回弹
- `1v1` 模式下一人控制双棋子
- 胜负判定边界

说明：

- “终点超出如何处理”已在规则稿中明确：精确停在终点才完成；支路上超出的点数从终点沿支路回退。

## 16. 性能与平台策略

### 16.1 PC / Web 首发建议

原因：

- Bevy Web 构建链成熟度高于移动端首期上线效率
- 当前项目核心在规则与体验验证，不在移动端商业化封装

### 16.2 Web 注意点

- 控制纹理尺寸和音频体积
- 避免首次加载包过大
- UI 文本与点击区域适配浏览器缩放

### 16.3 移动端第二阶段

- 适配触摸输入
- 调整 UI 安全区
- 减少同时播放的动画与粒子效果

## 17. 开发阶段规划

### 阶段 1：项目骨架

- 初始化 Bevy 项目
- 建立 `AppState` 与插件结构
- 搭建菜单切换
- 加载基础棋盘配置

交付物：

- 可启动项目
- 主菜单到游戏场景可切换

### 阶段 2：规则 MVP

- 实现棋盘路径
- 实现掷骰、出发、移动、撞击、终点
- 实现 `1v1` 与 `2v2`
- 实现回合流转

交付物：

- 无技能版本可完整对局

### 阶段 3：表现层

- 补齐 HUD
- 补齐移动/掷骰/撞击动画
- 补齐音效
- 增加快速模式

交付物：

- 一版可演示的完整体验

### 阶段 4：技能与事件

- 接入 3 到 5 个技能
- 接入防御点/攻击点/事件点
- 完成技能按钮与提示

交付物：

- 差异化玩法成立

### 阶段 5：AI 与调优

- 完成三档 AI
- 调平衡参数
- 增加策略提示开关

交付物：

- 单机可长期游玩版本

### 阶段 6：多平台适配

- Web 构建发布
- PC 打包
- 移动端输入和界面适配

## 18. 风险与规避

### 18.1 Bevy API 变更快

规避：

- 锁定 `0.18.x`
- 封装状态机、输入和资源加载层
- 避免过度依赖第三方插件

### 18.2 规则复杂度容易失控

规避：

- 先实现纯经典规则
- 技能与事件统一走 `GameAction`
- 新机制必须先补单测

### 18.3 AI 与动画耦合导致 Bug

规避：

- AI 只依赖逻辑快照
- 表现层只消费事件
- 逻辑推进不直接依赖 Transform

### 18.4 多模式增加状态分支

规避：

- 使用统一 `MatchConfig`
- `1v1` 与 `2v2` 尽量共用玩家/队伍抽象

## 19. MVP 结论

首个可交付版本建议定义为：

- 平台：PC + Web
- 模式：`1v1`、`2v2`
- 玩法：经典飞行棋核心规则
- 技能：至少 `Dash`、`Shield`、`DoubleDice`
- AI：简单 + 普通
- 表现：基础动画、基础音效、完整 HUD

不建议一开始就把“随机事件小游戏、复杂技能联动、本地 4 人同屏、移动端”同时纳入首期范围，否则节奏会被拖慢。

## 20. 下一步建议

按实施顺序，下一份落地文档建议是：

1. 详细规则文档
2. 棋盘数据格式文档
3. Rust crate 结构初始化
4. MVP 任务拆解表

如果直接进入开发，建议下一步先初始化 Bevy 工程骨架和模块目录，再补 `详细规则文档`，避免代码结构先天失衡。
