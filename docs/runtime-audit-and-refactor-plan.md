# 当前实现运行审计与优化改造计划

审计日期：2026-06-13

## 1. 已验证事实

- `cargo check` 通过，native 代码当前可编译。
- `./scripts/build-wasm.sh` 通过，`dist/` 下的 Web 构建可重新生成。
- `./scripts/serve-wasm.sh 8000` 可启动静态服务，浏览器访问 `http://127.0.0.1:8000/` 后 Bevy/WASM 初始化成功。
- 浏览器控制台未发现应用级 panic 或资源加载错误，仅有 Bevy/WebGL 能力降级日志。
- 手动跑通了主菜单、对局配置、进入棋盘、P1 掷骰、起飞、移动、AI 自动行动、技能触发、HUD 折叠、移动端宽度检查。
- `cargo test` 当前失败：48 个测试中 44 个通过，4 个失败，全部集中在队友叠加、共享护盾和护盾撞击回退。
- `cargo clippy --all-targets --all-features -- -D warnings` 当前失败：主要为系统函数参数过多、查询类型复杂，以及一个可用 `?` 简化的分支。

## 2. 运行时观察到的问题

### P0：规则测试红灯

失败用例：

- `apply_team_stack_grants_shared_shield_in_two_vs_two`
- `clear_stack_from_origin_removes_shared_shield_from_remaining_stack`
- `resolve_collision_consumes_shared_stack_shield_before_returning_to_hangar`
- `resolve_collision_with_shield_bounces_attacker_to_origin`

这些用例集中指向同一组规则：不同玩家的 `progress` 需要通过各自 `launch_tile_index` 映射到真实 `BoardPosition`，而测试数据和实际叠加/碰撞判定之间存在不一致。后续改造前必须先统一测试夹具和棋盘位置表达，否则很容易修出“测试通过但规则错误”的代码。

### P0：事件额外移动没有继续结算撞击

规则文档规定事件导致的额外前进可以触发撞击；当前 `AdvanceTwo` 事件只更新棋子进度和坐标，没有复用碰撞/叠加/胜负结算流水线。这会导致事件移动穿过或落到敌人时反馈缺失。

### P1：AI 第一轮攻击性过强且反馈弱

实测中 P2 AI 很早就自动使用 Snipe，把玩家刚起飞/移动的棋子打回机库。由于没有动画、没有独立事件日志、HUD 文本还被截断，玩家很难理解“棋子为什么消失”。AI 难度枚举存在，但当前策略没有真正按 Easy/Normal/Hard 分层。

### P1：技能边界和设计不一致

策划/规则里更像“每名玩家选择一个技能”，当前实现是每名玩家五个技能各有 1 次充能。实测 P1 没有明显 Active 棋子时也能使用 Shield，且没有棋子护盾视觉标记，规则边界和反馈都不够清楚。

### P1：HUD 与布局不可用风险高

桌面 1280x720 下 HUD 文本已经靠右截断；手机宽度下展开 HUD 主面板完全在屏幕外，只剩右上角折叠提示。当前 HUD 使用固定像素坐标，和 `docs/ui-design-reference.md` 的响应式目标不一致。

### P2：表现层仍是 MVP 骨架

移动、起飞、撞击和技能都是瞬时变化，没有移动动画、骰子动画、音效或关键事件提示。`AnimationPlugin` 和 `AudioPlugin` 目前仍是空入口。

### P2：代码结构开始胀大

Clippy 指出多个 Bevy system 参数超过阈值，`update_hud`、回合输入、AI 自动循环等函数承担了过多职责。后续继续加动画、事件日志和移动端 UI 时，维护成本会明显上升。

## 3. 优化改造计划

### 阶段 1：规则可信度修复

目标：让核心规则测试变绿，并保证测试表达真实棋盘格。

- 增加测试 helper：用 `BoardPosition::Main(index)` 反推各玩家 progress，避免手写进度造成假同格。
- 修正叠加、离开叠加、共享护盾消耗、单体护盾反弹的测试数据。
- 在规则层新增覆盖：同格队友叠加、共享盾优先消耗、护盾阻挡后攻击方回退、无盾成功撞击。
- 把事件 `AdvanceTwo` 接入一次受控的后续结算，至少覆盖额外移动后的撞击。

验收：

- `cargo test` 通过。
- 叠加/护盾相关测试明确断言 `BoardPosition`，不只断言 progress。

### 阶段 2：回合结算流水线重构

目标：减少特殊分支，让移动、事件移动、飞跃后落点都走同一套结算。

- 引入 `ActionResolutionContext` 或等价结构，收拢 `execute_action` 的长参数。
- 拆分 `MovementResolver`、`CollisionResolver`、`TileEffectResolver`、`VictoryResolver`。
- 让 `execute_action` 只负责编排，不直接塞满所有规则细节。
- 统一日志事件模型，输出结构化事件，再由 HUD/动画/音效消费。

验收：

- `execute_action` 参数数量下降。
- 事件移动和普通移动复用同一碰撞规则。
- `cargo clippy --all-targets --all-features -- -D warnings` 不再因当前结构问题失败。

### 阶段 3：技能与 AI 规则对齐

目标：让技能系统和策划口径一致，AI 行为可预测。

- 决定技能模型：每人一个技能，或保留多技能但更新规则文档和 UI。
- Shield 只允许合法目标，并在棋子上显示护盾层数。
- Snipe 增加目标选择/优先级限制，避免 AI 开局过度打断玩家体验。
- Easy/Normal/Hard 分层：Easy 随机，Normal 基础收益，Hard 才主动组合技能和攻击。

验收：

- AI 难度切换实际影响策略。
- 技能按钮和键盘入口遵循同一可用性判断。
- 玩家能从画面上看出技能是否生效。

### 阶段 4：HUD 与交互体验重做

目标：让游戏在桌面和窄屏都能看懂、能操作。

- 把 HUD 固定坐标改为基于窗口尺寸的布局资源。
- 桌面：棋盘 + 右侧信息栏，文本自动换行，不截断关键提示。
- 小屏：默认折叠为底部/顶部状态条，展开后使用覆盖面板或分页技能面板。
- 增加可见骰子按钮、当前玩家标识、候选棋子编号、最近事件列表。

验收：

- 1280x720 下 HUD 不截断关键文本。
- 390x844 下 HUD 展开态可见且可操作。
- 玩家不看控制台也能理解 AI、技能和撞击发生了什么。

### 阶段 5：表现与反馈补齐

目标：从“可验证规则”进入“可玩游戏”。

- 起飞、移动、撞击、回家增加短动画。
- 骰子加入视觉结果和滚动反馈。
- 关键事件绑定音效：掷骰、起飞、撞击、护盾抵挡、胜利。
- AI 行动增加短暂停顿和事件提示，而不是瞬间连跳。

验收：

- 完成一次 P1 -> P2 AI -> P3 -> P4 AI 回合时，玩家能逐步看懂发生了什么。
- 快速模式可以跳过或缩短动画，但不改变规则结果。

## 4. 建议优先级

下一步先做阶段 1 和阶段 2。原因是当前测试红灯和规则流水线不统一会污染后续所有 UI/AI/动画工作；先把规则可信度打牢，后面的体验优化才不会反复返工。

## 5. 实施结果记录

实施日期：2026-06-13

- 阶段 1：已修复队友叠加、共享护盾、护盾反弹测试数据，并让 `AdvanceTwo` 事件移动继续结算撞击。
- 阶段 2：已用参数上下文/SystemParam 收拢回合、技能和 HUD 系统参数，并清理 clippy 当前红灯。
- 阶段 3：技能模型决定保留首期五技能多充能方案；Shield 目标收紧为己方 Active 且未满盾棋子；AI 已按 Easy/Normal/Hard 分层，Hard Snipe 增加推进阈值。
- 阶段 4：HUD 改为右侧 UI 面板，支持小屏默认折叠、Tab 展开、Roll 按钮、候选棋子编号、最近事件列表与文本换行。
- 阶段 5：补齐棋子移动插值、护盾层数角标和基于 Bevy `Pitch` 的短音效反馈；快速模式会缩短移动动画。

最终验收：

- `cargo check` 通过。
- `cargo test` 通过，55 个测试全绿。
- `cargo clippy --all-targets --all-features -- -D warnings` 通过。
- `./scripts/build-wasm.sh` 通过。
- 浏览器烟测通过：1280x720 桌面 HUD 可读，390x844 小屏默认折叠且展开面板在屏内，未发现新的 console error。
